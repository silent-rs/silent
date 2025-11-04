use silent::{RateLimiterConfig, prelude::*};
use std::time::Duration;

fn main() {
    // 创建路由
    let router = Route::new("").get(|_req: Request| async {
        // 模拟一些处理时间
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok("Hello, World!")
    });

    // 配置限流器：
    // - 容量：5 个突发连接
    // - 每 100ms 补充 1 个令牌（相当于 10 QPS）
    // - 最多等待 2 秒获取令牌
    let rate_limiter_config = RateLimiterConfig {
        capacity: 5,
        refill_every: Duration::from_millis(100),
        max_wait: Duration::from_secs(2),
    };

    // 配置 Server 并启用限流
    Server::new()
        .bind("127.0.0.1:8080".parse().unwrap()).expect("Failed to bind to address")
        .with_rate_limiter(rate_limiter_config)
        // 配置优雅关停：等待 10 秒让连接完成
        .with_shutdown(Duration::from_secs(10))
        .on_listen(|addrs| {
            for addr in addrs {
                println!("Server listening on: {:?}", addr);
            }
            println!("Rate limiter: 5 burst capacity, ~10 QPS");
            println!("Try: curl http://127.0.0.1:8080/");
        })
        .run(router);
}
