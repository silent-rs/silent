use crate::{Request, Result, Scheduler, SilentError};
use async_lock::Mutex;
use http::StatusCode;
use std::sync::Arc;

pub trait SchedulerExt {
    fn scheduler(&self) -> Result<&Arc<Mutex<Scheduler>>>;
}

impl SchedulerExt for Request {
    fn scheduler(&self) -> Result<&Arc<Mutex<Scheduler>>> {
        self.extensions().get().ok_or_else(|| {
            SilentError::business_error(StatusCode::INTERNAL_SERVER_ERROR, "No scheduler found")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 成功场景测试 ====================

    #[test]
    fn test_scheduler_ext_success() {
        let mut req = Request::empty();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler.clone());

        let result = req.scheduler();
        assert!(result.is_ok());
        let retrieved_scheduler = result.unwrap();
        // 验证返回的是正确的调度器引用
        assert!(Arc::ptr_eq(retrieved_scheduler, &scheduler));
    }

    #[test]
    fn test_scheduler_ext_multiple_calls() {
        let mut req = Request::empty();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler.clone());

        let result1 = req.scheduler();
        let result2 = req.scheduler();

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        let scheduler1 = result1.unwrap();
        let scheduler2 = result2.unwrap();

        // 多次调用应该返回相同的调度器引用
        assert!(Arc::ptr_eq(scheduler1, scheduler2));
        assert!(Arc::ptr_eq(scheduler1, &scheduler));
    }

    #[test]
    fn test_scheduler_ext_with_global_scheduler() {
        let mut req = Request::empty();
        let global_scheduler = crate::scheduler::SCHEDULER.clone();
        req.extensions_mut().insert(global_scheduler);

        let result = req.scheduler();
        assert!(result.is_ok());
        let retrieved_scheduler = result.unwrap();
        // 验证返回的是全局调度器
        assert!(Arc::ptr_eq(
            retrieved_scheduler,
            &crate::scheduler::SCHEDULER
        ));
    }

    // ==================== 错误场景测试 ====================

    #[test]
    fn test_scheduler_ext_no_scheduler() {
        let req = Request::empty();

        let result = req.scheduler();
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduler_ext_error_status_code() {
        let req = Request::empty();

        let result = req.scheduler();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_scheduler_ext_error_message() {
        let req = Request::empty();

        let result = req.scheduler();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message().contains("No scheduler found"));
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_ext_with_middleware() {
        use crate::Handler;
        use crate::route::Route;
        use crate::scheduler::middleware::SchedulerMiddleware;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler = req.scheduler();
                assert!(scheduler.is_ok());
                let retrieved_scheduler = scheduler.unwrap();
                // 验证是全局调度器
                assert!(Arc::ptr_eq(
                    retrieved_scheduler,
                    &crate::scheduler::SCHEDULER
                ));
                Ok("scheduler found")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: crate::Result<crate::Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_ext_without_middleware() {
        use crate::Handler;
        use crate::route::Route;

        let route = Route::new("/").get(|req: Request| async move {
            let scheduler = req.scheduler();
            assert!(scheduler.is_err());
            assert_eq!(
                scheduler.unwrap_err().status(),
                StatusCode::INTERNAL_SERVER_ERROR
            );
            Ok("no scheduler")
        });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: crate::Result<crate::Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_scheduler_ext_scheduler_removed() {
        let mut req = Request::empty();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler.clone());

        // 验证调度器存在
        let result1 = req.scheduler();
        assert!(result1.is_ok());

        // 移除调度器
        req.extensions_mut().remove::<Arc<Mutex<Scheduler>>>();

        // 验证调度器不存在
        let result2 = req.scheduler();
        assert!(result2.is_err());
        assert_eq!(
            result2.unwrap_err().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_scheduler_ext_scheduler_replaced() {
        let mut req = Request::empty();
        let scheduler1 = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler1.clone());

        // 验证第一个调度器
        let result1 = req.scheduler();
        assert!(result1.is_ok());
        assert!(Arc::ptr_eq(result1.unwrap(), &scheduler1));

        // 替换调度器
        let scheduler2 = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler2.clone());

        // 验证第二个调度器
        let result2 = req.scheduler();
        assert!(result2.is_ok());
        let retrieved2 = result2.unwrap();
        assert!(Arc::ptr_eq(retrieved2, &scheduler2));
        // 确保不是第一个调度器
        assert!(!Arc::ptr_eq(retrieved2, &scheduler1));
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_ext_with_different_http_methods() {
        use crate::Handler;
        use crate::route::Route;
        use crate::scheduler::middleware::SchedulerMiddleware;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler = req.scheduler();
                assert!(scheduler.is_ok());
                Ok("GET")
            })
            .post(|req: Request| async move {
                let scheduler = req.scheduler();
                assert!(scheduler.is_ok());
                Ok("POST")
            })
            .put(|req: Request| async move {
                let scheduler = req.scheduler();
                assert!(scheduler.is_ok());
                Ok("PUT")
            });

        let route = Route::new_root().append(route);

        // 测试 GET
        let mut req = Request::empty();
        *req.method_mut() = http::Method::GET;
        let result: crate::Result<crate::Response> = route.call(req).await;
        assert!(result.is_ok());

        // 测试 POST
        let mut req = Request::empty();
        *req.method_mut() = http::Method::POST;
        let result: crate::Result<crate::Response> = route.call(req).await;
        assert!(result.is_ok());

        // 测试 PUT
        let mut req = Request::empty();
        *req.method_mut() = http::Method::PUT;
        let result: crate::Result<crate::Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_ext_concurrent_requests() {
        use crate::Handler;
        use crate::route::Route;
        use crate::scheduler::middleware::SchedulerMiddleware;
        use std::sync::Arc;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler = req.scheduler();
                assert!(scheduler.is_ok());
                Ok("concurrent")
            });

        let route: Arc<Route> = Arc::new(Route::new_root().append(route));

        // 并发多个请求
        let tasks = (0..5).map(|_| {
            let route = Arc::clone(&route);
            tokio::spawn(async move {
                let req = Request::empty();
                let result: crate::Result<crate::Response> = route.call(req).await;
                result
            })
        });

        for task in tasks {
            let result = task.await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_scheduler_ext_lifetime() {
        // 验证返回的引用的生命周期正确绑定到 self
        let mut req = Request::empty();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler.clone());

        let scheduler_ref = req.scheduler().unwrap();
        // scheduler_ref 的生命周期应该与 req 相关联
        assert!(Arc::ptr_eq(scheduler_ref, &scheduler));
        // 在 req 仍然存在时，scheduler_ref 应该有效
        drop(scheduler);
        assert!(Arc::ptr_eq(
            scheduler_ref,
            req.extensions().get::<Arc<Mutex<Scheduler>>>().unwrap()
        ));
    }

    #[test]
    fn test_scheduler_ext_scheduler_mutability() {
        let mut req = Request::empty();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        req.extensions_mut().insert(scheduler);

        let scheduler_ref = req.scheduler().unwrap();
        // 验证可以通过获取的引用访问调度器
        // 注意：这里只验证能获取引用，不实际修改以避免影响其他测试
        assert!(Arc::ptr_eq(
            scheduler_ref,
            req.extensions().get::<Arc<Mutex<Scheduler>>>().unwrap()
        ));
    }
}
