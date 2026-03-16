use std::sync::{Arc, Mutex};

use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError, StatusCode};
use async_trait::async_trait;
use http::header::RETRY_AFTER;

/// 令牌桶内部状态
struct BucketState {
    tokens: f64,
    last_refill: std::time::Instant,
}

/// RateLimiter 中间件
///
/// 基于令牌桶算法的路由/API 级别限流中间件。与连接级 `RateLimiterConfig` 不同，
/// 此中间件作用于请求处理层，可以挂载在任意路由节点上。
///
/// # 行为
///
/// 1. 每个请求到达时尝试从令牌桶中消耗 1 个令牌
/// 2. 如果令牌不足，返回 `429 Too Many Requests`，并设置 `Retry-After` 头
/// 3. 令牌按配置的速率持续补充
///
/// # 参数
///
/// - `rate`: 每秒补充的令牌数（即允许的平均 QPS）
/// - `capacity`: 令牌桶容量（允许的最大突发请求数）
///
/// # 示例
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::RateLimiter;
///
/// // 每秒 10 个请求，最大突发 20 个
/// let route = Route::new("/api")
///     .hook(RateLimiter::new(10.0, 20))
///     .get(|_req: Request| async { Ok("ok") });
/// ```
///
/// 每秒 1 个请求，无突发：
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::RateLimiter;
///
/// let route = Route::new("/api")
///     .hook(RateLimiter::per_second(1.0))
///     .get(|_req: Request| async { Ok("ok") });
/// ```
#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<BucketState>>,
    rate: f64,
    capacity: usize,
}

impl RateLimiter {
    /// 创建限流中间件。
    ///
    /// - `rate`: 每秒补充的令牌数（平均 QPS）
    /// - `capacity`: 令牌桶容量（突发上限）
    pub fn new(rate: f64, capacity: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(BucketState {
                tokens: capacity as f64,
                last_refill: std::time::Instant::now(),
            })),
            rate,
            capacity,
        }
    }

    /// 创建仅设置速率的限流中间件，容量等于速率向上取整。
    pub fn per_second(rate: f64) -> Self {
        Self::new(rate, rate.ceil() as usize)
    }

    /// 尝试消耗一个令牌，返回是否成功。
    fn try_acquire(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();

        // 补充令牌
        if elapsed > 0.0 {
            state.tokens = (state.tokens + elapsed * self.rate).min(self.capacity as f64);
            state.last_refill = now;
        }

        // 尝试消耗
        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// 计算下一个令牌可用的等待秒数。
    fn retry_after_secs(&self) -> u64 {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let deficit = 1.0 - state.tokens;
        if deficit <= 0.0 {
            return 0;
        }
        (deficit / self.rate).ceil() as u64
    }
}

#[async_trait]
impl MiddleWareHandler for RateLimiter {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        if self.try_acquire() {
            next.call(req).await
        } else {
            let retry_after = self.retry_after_secs().max(1);
            tracing::debug!(retry_after, "rate limit exceeded");
            let mut err = SilentError::business_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too Many Requests".to_string(),
            );
            if let SilentError::BusinessError { .. } = &mut err {
                // 在错误响应中无法直接设置头，通过返回带头的 Response 实现
            }
            // 构造带 Retry-After 头的 429 响应
            let mut res = Response::empty();
            res.set_status(StatusCode::TOO_MANY_REQUESTS);
            res.headers_mut()
                .insert(RETRY_AFTER, retry_after.to_string().parse().unwrap());
            res.set_body(crate::core::res_body::full("Too Many Requests"));
            Ok(res)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_rate_limiter_new() {
        let rl = RateLimiter::new(10.0, 20);
        assert_eq!(rl.rate, 10.0);
        assert_eq!(rl.capacity, 20);
    }

    #[test]
    fn test_rate_limiter_per_second() {
        let rl = RateLimiter::per_second(5.0);
        assert_eq!(rl.rate, 5.0);
        assert_eq!(rl.capacity, 5);
    }

    #[test]
    fn test_rate_limiter_per_second_fractional() {
        let rl = RateLimiter::per_second(1.5);
        assert_eq!(rl.rate, 1.5);
        assert_eq!(rl.capacity, 2);
    }

    #[test]
    fn test_rate_limiter_clone() {
        let rl1 = RateLimiter::new(10.0, 20);
        let rl2 = rl1.clone();
        assert_eq!(rl1.rate, rl2.rate);
        assert_eq!(rl1.capacity, rl2.capacity);
        // 克隆共享同一个状态
        assert!(Arc::ptr_eq(&rl1.state, &rl2.state));
    }

    // ==================== try_acquire 测试 ====================

    #[test]
    fn test_try_acquire_success() {
        let rl = RateLimiter::new(10.0, 5);
        // 初始有 5 个令牌
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
    }

    #[test]
    fn test_try_acquire_exhausted() {
        let rl = RateLimiter::new(10.0, 2);
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        // 令牌耗尽
        assert!(!rl.try_acquire());
    }

    #[test]
    fn test_try_acquire_refill() {
        let rl = RateLimiter::new(1000.0, 1);
        assert!(rl.try_acquire());
        assert!(!rl.try_acquire());
        // 等待令牌补充
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(rl.try_acquire());
    }

    #[test]
    fn test_try_acquire_capacity_cap() {
        let rl = RateLimiter::new(10000.0, 3);
        // 等待足够长时间让令牌补充满
        std::thread::sleep(std::time::Duration::from_millis(10));
        // 消耗 3 个应该可以
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        // 第 4 个应该失败（受容量限制）
        assert!(!rl.try_acquire());
    }

    // ==================== retry_after_secs 测试 ====================

    #[test]
    fn test_retry_after_secs_with_tokens() {
        let rl = RateLimiter::new(10.0, 5);
        assert_eq!(rl.retry_after_secs(), 0);
    }

    #[test]
    fn test_retry_after_secs_exhausted() {
        let rl = RateLimiter::new(1.0, 1);
        rl.try_acquire(); // 消耗唯一的令牌
        let retry = rl.retry_after_secs();
        assert!(retry >= 1);
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_rate_limiter_allows_request() {
        use crate::route::Route;

        let rl = RateLimiter::new(100.0, 10);
        let route = Route::new("/")
            .hook(rl)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_rate_limiter_blocks_excess() {
        use crate::route::Route;

        let rl = RateLimiter::new(0.001, 1); // 极低速率
        let route = Route::new("/")
            .hook(rl)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        // 第一个请求成功
        let req1 = Request::empty();
        let res1: Result<Response> = crate::Handler::call(&route, req1).await;
        assert!(res1.is_ok());
        assert_eq!(res1.unwrap().status(), StatusCode::OK);

        // 第二个请求被限流
        let req2 = Request::empty();
        let res2: Result<Response> = crate::Handler::call(&route, req2).await;
        assert!(res2.is_ok());
        let resp2 = res2.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp2.headers().contains_key(RETRY_AFTER));
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_rate_limiter_shared_across_clones() {
        use crate::route::Route;
        use std::sync::Arc;

        let rl = RateLimiter::new(0.001, 2);
        let route = Route::new("/")
            .hook(rl)
            .get(|_req: Request| async { Ok("ok") });
        let route = Arc::new(Route::new_root().append(route));

        // 消耗两个令牌
        let req1 = Request::empty();
        let res1: Result<Response> = crate::Handler::call(&*route, req1).await;
        assert_eq!(res1.unwrap().status(), StatusCode::OK);

        let req2 = Request::empty();
        let res2: Result<Response> = crate::Handler::call(&*route, req2).await;
        assert_eq!(res2.unwrap().status(), StatusCode::OK);

        // 第三个请求被限流
        let req3 = Request::empty();
        let res3: Result<Response> = crate::Handler::call(&*route, req3).await;
        assert_eq!(res3.unwrap().status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_rate_limiter_concurrent() {
        use crate::route::Route;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let rl = RateLimiter::new(0.001, 5); // 5 个令牌，极低补充速率
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let route = Route::new("/").hook(rl).get(move |_req: Request| {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok("ok")
            }
        });
        let route = Arc::new(Route::new_root().append(route));

        let mut tasks = Vec::new();
        for _ in 0..10 {
            let route = Arc::clone(&route);
            tasks.push(tokio::spawn(async move {
                let req = Request::empty();
                let res: Result<Response> = crate::Handler::call(&*route, req).await;
                res.unwrap().status()
            }));
        }

        let mut ok_count = 0;
        let mut limited_count = 0;
        for task in tasks {
            match task.await.unwrap() {
                StatusCode::OK => ok_count += 1,
                StatusCode::TOO_MANY_REQUESTS => limited_count += 1,
                _ => panic!("unexpected status"),
            }
        }

        // 应该有 5 个通过，5 个被限流
        assert_eq!(ok_count, 5);
        assert_eq!(limited_count, 5);
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }
}
