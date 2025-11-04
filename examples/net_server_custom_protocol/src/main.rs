use silent::{BoxedConnection, NetServer, RateLimiterConfig, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// 自定义行分隔命令协议处理器
/// 支持命令：
///   PING          -> 返回 PONG
///   ECHO <msg>    -> 返回 <msg>
///   QUIT          -> 关闭连接
async fn handle_protocol(
    stream: BoxedConnection,
    peer: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("New connection from {:?}", peer);

    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    // 欢迎消息
    writer
        .write_all(b"Welcome! Commands: PING, ECHO <msg>, QUIT\n")
        .await?;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        tracing::info!("Received from {:?}: {}", peer, line);

        if line.eq_ignore_ascii_case("PING") {
            writer.write_all(b"PONG\n").await?;
        } else if line.eq_ignore_ascii_case("QUIT") {
            writer.write_all(b"Goodbye!\n").await?;
            break;
        } else if let Some(msg) = line.strip_prefix("ECHO ") {
            writer.write_all(msg.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        } else {
            writer
                .write_all(b"Unknown command. Try: PING, ECHO <msg>, QUIT\n")
                .await?;
        }
    }

    tracing::info!("Connection closed by {:?}", peer);
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let rate_limiter_config = RateLimiterConfig {
        capacity: 5,                              // 容量：最多 5 个并发连接
        refill_every: Duration::from_millis(200), // 每 200ms 补充 1 个令牌（~5 QPS）
        max_wait: Duration::from_secs(3),         // 获取令牌最多等待 3 秒
    };

    let server = NetServer::new()
        .bind("127.0.0.1:18081".parse().unwrap()).expect("Failed to bind to address")
        .with_rate_limiter(rate_limiter_config)
        .with_shutdown(Duration::from_secs(10)) // 关停时等待 10 秒
        .on_listen(|addrs| {
            tracing::info!("Custom protocol server listening on: {:?}", addrs);
            println!("Try: echo 'PING' | nc 127.0.0.1 18081");
            println!("Try: echo 'ECHO Hello World' | nc 127.0.0.1 18081");
        });

    server.serve(handle_protocol).await;
}
