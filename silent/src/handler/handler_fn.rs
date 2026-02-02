use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;

use crate::{Handler, Request, Response, Result};

/// 泛型处理器包装器：让直接传入闭包保持静态分发，不再经过额外的 HandlerWrapper。
pub struct HandlerFn<F> {
    func: F,
}

impl<F> HandlerFn<F> {
    pub fn new(func: F) -> Self {
        Self { func }
    }

    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl<F, Fut> Handler for HandlerFn<F>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    async fn call(&self, req: Request) -> Result<Response> {
        let resp = (self.func)(req).await;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_handler_fn_new() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("test") });
        // 验证 HandlerFn 被正确创建
        let _ = handler;
    }

    #[test]
    fn test_handler_fn_closure() {
        // 测试不同类型的闭包
        let handler1 = HandlerFn::new(|_req: Request| async { Response::text("handler1") });
        let handler2 = HandlerFn::new(|_req: Request| async {
            Response::json(&serde_json::json!({"key": "value"}))
        });

        let _ = handler1;
        let _ = handler2;
    }

    // ==================== arc() 方法测试 ====================

    #[test]
    fn test_handler_fn_arc() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("test") });
        let arc_handler = handler.arc();

        // 验证返回的是 Arc
        let _ = Arc::into_raw(arc_handler);
    }

    #[test]
    fn test_handler_fn_arc_clone() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("test") });
        let arc_handler = handler.arc();

        // Arc 可以被克隆
        let _clone = arc_handler.clone();
    }

    #[test]
    fn test_handler_fn_arc_shared() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("test") });
        let arc_handler = handler.arc();

        // 多个 Arc 引用指向同一个对象
        let arc1 = Arc::clone(&arc_handler);
        let arc2 = Arc::clone(&arc_handler);

        let raw_ptr1 = Arc::into_raw(arc1) as *const ();
        let raw_ptr2 = Arc::into_raw(arc2) as *const ();
        let raw_ptr3 = Arc::into_raw(arc_handler) as *const ();

        // 所有指针应该相同
        assert_eq!(raw_ptr1, raw_ptr2);
        assert_eq!(raw_ptr2, raw_ptr3);
    }

    // ==================== Handler trait call() 方法测试 ====================

    #[tokio::test]
    async fn test_handler_fn_call_text() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("hello") });
        let req = Request::empty();

        let result = handler.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_handler_fn_call_json() {
        let handler = HandlerFn::new(|_req: Request| async {
            Response::json(&serde_json::json!({"message": "test"}))
        });
        let req = Request::empty();

        let result = handler.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_handler_fn_call_html() {
        let handler = HandlerFn::new(|_req: Request| async { Response::html("<h1>Hello</h1>") });
        let req = Request::empty();

        let result = handler.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_handler_fn_call_empty() {
        let handler = HandlerFn::new(|_req: Request| async { Response::empty() });
        let req = Request::empty();

        let result = handler.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_handler_fn_call_with_request_data() {
        let handler = HandlerFn::new(|req: Request| async move {
            // 使用请求中的数据
            let method = req.method().to_string();
            Response::text(&format!("Method: {}", method))
        });

        let mut req = Request::empty();
        *req.method_mut() = http::Method::POST;

        let result = handler.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_handler_fn_call_arc() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("test") });
        let arc_handler = handler.arc();
        let req = Request::empty();

        let result = arc_handler.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    // ==================== 异步行为测试 ====================

    #[tokio::test]
    async fn test_handler_fn_async_delay() {
        let handler = HandlerFn::new(|_req: Request| async {
            // 模拟异步操作
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Response::text("async response")
        });

        let req = Request::empty();
        let result = handler.call(req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handler_fn_concurrent_calls() {
        let handler = HandlerFn::new(|_req: Request| async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Response::text("concurrent")
        });

        let arc_handler = Arc::new(handler);

        // 并发调用
        let task1 = arc_handler.call(Request::empty());
        let task2 = arc_handler.call(Request::empty());
        let task3 = arc_handler.call(Request::empty());

        let (result1, result2, result3) = tokio::join!(task1, task2, task3);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert!(result3.is_ok());
    }

    // ==================== Trait 边界测试 ====================

    #[test]
    fn test_handler_fn_send_sync() {
        // 验证 HandlerFn 满足 Send + Sync 约束
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<HandlerFn<fn(Request) -> std::future::Ready<Response>>>();
    }

    #[test]
    fn test_handler_fn_static() {
        // 验证闭包可以捕获 'static 生命周期
        let text = "static text";
        let handler = HandlerFn::new(move |_req: Request| async { Response::text(text) });

        let _ = handler;
    }

    // ==================== 类型安全测试 ====================

    #[tokio::test]
    async fn test_handler_fn_return_type() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("typed response") });

        let req = Request::empty();
        let result: Result<Response> = handler.call(req).await;

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_handler_fn_error_propagation() {
        let handler = HandlerFn::new(|_req: Request| async {
            // 正常情况返回 Response
            Response::text("no error")
        });

        let req = Request::empty();
        let result = handler.call(req).await;

        // HandlerFn 的 call() 总是返回 Ok(Response)
        assert!(result.is_ok());
    }

    // ==================== 不同闭包形式测试 ====================

    #[tokio::test]
    async fn test_handler_fn_with_captured_variable() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));

        // 避免在 async 块中引用捕获的变量
        let counter_clone = Arc::clone(&counter);
        let handler = HandlerFn::new(move |_req: Request| {
            let counter = Arc::clone(&counter_clone);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Response::text("captured")
            }
        });

        let arc_handler = Arc::new(handler);

        // 调用多次
        for _ in 0..3 {
            let _ = arc_handler.call(Request::empty()).await;
        }

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_handler_fn_with_move_closure() {
        let data = vec![1, 2, 3, 4, 5];

        let handler = HandlerFn::new(move |_req: Request| {
            let data = data.clone();
            async move {
                let sum: i32 = data.iter().sum();
                Response::text(&format!("Sum: {}", sum))
            }
        });

        let req = Request::empty();
        let result = handler.call(req).await;

        assert!(result.is_ok());
    }

    // ==================== 边界条件和特殊情况 ====================

    #[tokio::test]
    async fn test_handler_fn_empty_response() {
        let handler = HandlerFn::new(|_req: Request| async {
            let mut response = Response::empty();
            response.set_status(http::StatusCode::NO_CONTENT);
            response
        });

        let req = Request::empty();
        let result = handler.call(req).await;

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 204);
    }

    #[tokio::test]
    async fn test_handler_fn_large_response() {
        let handler = HandlerFn::new(|_req: Request| async {
            let large_text = "x".repeat(10000);
            Response::text(&large_text)
        });

        let req = Request::empty();
        let result = handler.call(req).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handler_fn_unicode_response() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("你好世界 🌍") });

        let req = Request::empty();
        let result = handler.call(req).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handler_fn_different_methods() {
        let handler = HandlerFn::new(|req: Request| async move {
            let method = req.method().to_string();
            let text = format!("Method: {}", method);
            Response::text(&text)
        });

        // 测试不同的 HTTP 方法
        for method in &[http::Method::GET, http::Method::POST, http::Method::PUT] {
            let mut req = Request::empty();
            *req.method_mut() = method.clone();

            let result = handler.call(req).await;
            assert!(result.is_ok());
        }
    }

    // ==================== 性能和资源测试 ====================

    #[tokio::test]
    async fn test_handler_fn_multiple_arc_calls() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("performance test") });

        let arc_handler = Arc::new(handler);

        // 多次调用
        for _ in 0..100 {
            let req = Request::empty();
            let result = arc_handler.call(req).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_handler_fn_no_memory_leak() {
        let handler = HandlerFn::new(|_req: Request| async { Response::text("memory test") });

        let arc_handler = Arc::new(handler);

        // 创建多个请求
        for _ in 0..10 {
            let req = Request::empty();
            let _ = arc_handler.call(req).await;
        }

        // Arc 引用计数应该正常
        assert_eq!(Arc::strong_count(&arc_handler), 1);
    }
}
