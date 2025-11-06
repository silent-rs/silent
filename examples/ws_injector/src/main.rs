use futures::channel::oneshot;
use tokio_util::compat::TokioAsyncReadCompatExt;

use silent::ws::upgrade::{AsyncUpgradeRx, on_generic};
use tokio::io::DuplexStream;

// 演示：在非 server（如 wasm/WASI）环境，注入一个已“升级”的连接到 Request.extensions，
// 再通过 on_generic 提取并交由上层构建 WebSocket。
#[tokio::main]
async fn main() {
    // 1) 构造一个假的“已升级”连接，这里用 tokio::io::duplex 模拟
    let (_client, server_side): (DuplexStream, DuplexStream) = tokio::io::duplex(64);
    // 将 Tokio IO 适配为 futures-io
    let compat_stream = server_side.compat();

    // 2) 建立 oneshot 通道，并把接收端封装为 AsyncUpgradeRx 注入到 Request.extensions
    let (tx, rx) = oneshot::channel();
    let mut req = silent::Request::default();
    req.extensions_mut().insert(AsyncUpgradeRx::new(rx));

    // 模拟宿主在握手完成后，将底层流发送到 upgrade 通道
    let _ = tx.send(compat_stream);

    // 3) 在应用侧，从 Request 提取“已升级”的连接（类型由注入决定）
    let upgraded = on_generic::<tokio_util::compat::Compat<DuplexStream>>(req)
        .await
        .expect("upgrade ok");

    // 说明：此处仅演示拿到 Upgraded<S>，实际可继续：
    // let ws = silent::ws::WebSocket::from_raw_socket(upgraded, protocol::Role::Server, None).await;
    // 再配合 WebSocketHandler 调用 ws.handle(handler).await
    println!(
        "on_generic extracted upgraded stream with parts: {:?}",
        upgraded.path_params()
    );
}
