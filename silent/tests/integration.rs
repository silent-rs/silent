use silent::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// 计数器中间件，用于测试中间件执行顺序
#[derive(Clone)]
struct CounterMiddleware {
    name: String,
    counter: Arc<AtomicUsize>,
}

impl CounterMiddleware {
    fn new(name: &str, counter: Arc<AtomicUsize>) -> Self {
        Self {
            name: name.to_string(),
            counter,
        }
    }
}

#[async_trait::async_trait]
impl MiddleWareHandler for CounterMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> silent::Result<Response> {
        let count = self.counter.fetch_add(1, Ordering::SeqCst);
        println!(
            "🔧 {} middleware executed (count: {})",
            self.name,
            count + 1
        );

        let response = next.call(req).await?;
        println!("🔧 {} middleware finished", self.name);
        Ok(response)
    }
}

async fn user_handler(_req: Request) -> silent::Result<String> {
    println!("📍 Handler executed: user");
    Ok("User handler".to_string())
}

#[tokio::test]
async fn test_layered_middleware_execution() {
    let counter = Arc::new(AtomicUsize::new(0));

    // 创建简单的中间件测试
    let middleware1 = CounterMiddleware::new("LEVEL1", counter.clone());
    let middleware2 = CounterMiddleware::new("LEVEL2", counter.clone());

    // 构建简单的路由结构
    let app = Route::new("api")
        .hook(middleware1) // 第一层中间件
        .append(
            Route::new("users")
                .hook(middleware2) // 第二层中间件
                .get(user_handler),
        );

    let mut root_route = Route::new_root();
    root_route.push(app);

    // 测试 /api/users - 应该执行两个中间件
    println!("\n=== 测试 GET /api/users ===");
    counter.store(0, Ordering::SeqCst);

    let mut req = Request::empty();
    req.set_remote("127.0.0.1:8080".parse().unwrap());
    *req.uri_mut() = "/api/users".parse().unwrap();
    *req.method_mut() = http::Method::GET;

    let _response = root_route.call(req).await.unwrap();
    // 验证中间件确实被执行了
    assert!(counter.load(Ordering::SeqCst) > 0, "中间件应该被执行");

    println!("\n✅ 中间件层级测试通过！");
}

#[tokio::test]
async fn test_middleware_independence() {
    // 测试每个路由层级的中间件独立性
    let counter = Arc::new(AtomicUsize::new(0));
    let middleware1 = CounterMiddleware::new("LEVEL1", counter.clone());
    let middleware2 = CounterMiddleware::new("LEVEL2", counter.clone());

    let route = Route::new("api")
        .hook(middleware1)
        .append(Route::new("users").hook(middleware2).get(user_handler));

    // 验证父路由和子路由有各自独立的中间件
    assert_eq!(route.middlewares.len(), 1, "父路由应该有1个中间件");
    assert_eq!(
        route.children[0].middlewares.len(),
        1,
        "子路由应该有1个中间件"
    );

    println!("✅ 中间件独立性测试通过！");
}
