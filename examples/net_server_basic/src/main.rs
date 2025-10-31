use silent::{BoxedConnection, NetServer, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 创建简单的回显处理器
    let handler = |mut stream: BoxedConnection, peer: SocketAddr| async move {
        tracing::info!("New connection from {:?}", peer);

        let mut buf = vec![0u8; 1024];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => {
                    tracing::info!("Connection closed by {:?}", peer);
                    break;
                }
                Ok(n) => {
                    tracing::info!("Received {} bytes from {:?}", n, peer);
                    // 回显数据
                    if let Err(e) = stream.write_all(&buf[..n]).await {
                        tracing::error!("Failed to write to {:?}: {:?}", peer, e);
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to read from {:?}: {:?}", peer, e);
                    break;
                }
            }
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    };

    // 配置 NetServer：限流 10 QPS，优雅关停等待 5 秒
    let server = NetServer::new()
        .bind("127.0.0.1:18080".parse().unwrap())
        .with_rate_limiter(
            10,                         // 容量：同时最多 10 个连接
            Duration::from_millis(100), // 每 100ms 补充 1 个令牌（~10 QPS）
            Duration::from_secs(2),     // 获取令牌最多等待 2 秒
        )
        .with_shutdown(Duration::from_secs(5)) // 关停时等待 5 秒
        .on_listen(|addrs| {
            tracing::info!("NetServer listening on: {:?}", addrs);
            println!("Try: echo 'hello' | nc 127.0.0.1 18080");
        });

    server.serve(handler).await;
}
