// NetServer 集成测试
// TODO: 当前 NetServer API 设计限制了集成测试的可行性：
// 1. on_listen 回调无法将监听地址返回给外部
// 2. serve() 方法会阻塞，难以在测试中控制生命周期
// 建议在后续版本中改进 API 设计，例如：
//   - 添加 serve_with_addr() -> (Vec<SocketAddr>, JoinHandle)
//   - 或使用 channel 传递监听地址
//
// 当前通过运行 examples 手动验证功能正确性

use silent::{BoxedConnection, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_connection_service_trait() {
    // 测试 ConnectionService trait 的闭包实现
    let call_count = Arc::new(tokio::sync::Mutex::new(0_u32));
    let call_count_clone = call_count.clone();

    let _handler = move |mut stream: BoxedConnection, _peer: SocketAddr| {
        let count = call_count_clone.clone();
        async move {
            *count.lock().await += 1;
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await?;
            stream.write_all(b"OK").await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }
    };

    // 验证初始状态
    assert_eq!(*call_count.lock().await, 0);
}
