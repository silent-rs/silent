use silent::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// 计数器中间件，用于演示中间件执行顺序
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
        println!("🔧 {} middleware executed (count: {})", self.name, count + 1);

        let response = next.call(req).await?;
        println!("🔧 {} middleware finished", self.name);
        Ok(response)
    }
}

async fn hello(_req: Request) -> Result<String, SilentError> {
    println!("📍 Handler executed: hello");
    Ok("Hello from /api/v1/hello".to_string())
}

async fn world(_req: Request) -> Result<String, SilentError> {
    println!("📍 Handler executed: world");
    Ok("World from /api/v1/world".to_string())
}

async fn user_handler(_req: Request) -> Result<String, SilentError> {
    println!("📍 Handler executed: user");
    Ok("User handler".to_string())
}

async fn root_handler(_req: Request) -> Result<String, SilentError> {
    println!("📍 Handler executed: root");
    Ok("Root page".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let counter = Arc::new(AtomicUsize::new(0));

    // 创建不同层级的中间件
    let root_middleware = CounterMiddleware::new("ROOT", counter.clone());
    let api_middleware = CounterMiddleware::new("API", counter.clone());
    let v1_middleware = CounterMiddleware::new("V1", counter.clone());
    let users_middleware = CounterMiddleware::new("USERS", counter.clone());

    // 构建路由结构，每一层都有自己的中间件
    let app = Route::new("")
        .hook(root_middleware)  // 根级中间件
        .get(root_handler)
        .append(
            Route::new("api")
                .hook(api_middleware)  // API级中间件
                .append(
                    Route::new("v1")
                        .hook(v1_middleware)  // V1级中间件
                        .get(hello)
                        .post(world)
                        .append(
                            Route::new("users")
                                .hook(users_middleware)  // Users级中间件
                                .get(user_handler)
                        )
                )
        );

    println!("🚀 启动层级中间件演示服务器...");
    println!("📋 测试用例:");
    println!("   GET  /                - 应该执行: ROOT middleware");
    println!("   GET  /api/v1/hello    - 应该执行: ROOT -> API -> V1 middleware");
    println!("   POST /api/v1/world    - 应该执行: ROOT -> API -> V1 middleware");
    println!("   GET  /api/v1/users    - 应该执行: ROOT -> API -> V1 -> USERS middleware");
    println!("");
    println!("💡 每个路由层级独立管理自己的中间件");
    println!("💡 匹配到路由后，会按层级顺序执行所有相关中间件");
    println!("");

    let mut root_route = RootRoute::new();
    root_route.push(app);

    let addr = "127.0.0.1:3000".parse()?;
    Server::new().bind(addr).serve(root_route).await;

    Ok(())
}
