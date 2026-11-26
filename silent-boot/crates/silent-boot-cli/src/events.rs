//! 开发期的事件服务器：浏览器连上 `/__dev__/events` 后，后端重新构建并重启时会收到 `reload`。
//!
//! 协议是 SSE，用裸 TCP 实现，不依赖 Web 框架。`/__dev__/reload.js` 返回一小段脚本，
//! 页面里引一个 `<script>` 就能在后端重启后自动刷新，构建失败时在页面上显示错误。
//!
//! 事件服务器和被重启的应用是两个进程，所以重启应用不会断开浏览器的连接。

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

const SSE_HEADERS: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";

/// 事件服务器。丢弃时会停掉监听，已经连上的页面会断开。
pub struct Events {
    tx: broadcast::Sender<Arc<String>>,
    addr: SocketAddr,
    // 只为了持有它：丢弃时连带停掉监听。
    _accept_task: tokio::task::JoinHandle<()>,
}

impl Events {
    /// 在 `addr` 上监听。端口填 0 可以让系统分配，再用 [`Events::addr`] 取出来。
    pub async fn bind(addr: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let (tx, _) = broadcast::channel(64);
        let accept = tx.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = accept.clone();
                        tokio::spawn(serve(stream, tx));
                    }
                    Err(e) => {
                        eprintln!("[dev] 接受连接失败：{e}");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });
        Ok(Self {
            tx,
            addr,
            _accept_task: accept_task,
        })
    }

    /// 实际监听的地址。
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// 页面里要引用的脚本地址。
    pub fn script_url(&self) -> String {
        format!("http://{}/__dev__/reload.js", self.addr)
    }

    /// 给所有连上的页面发一个事件。`data` 可以有多行。
    pub fn publish(&self, event: &str, data: &str) {
        let mut frame = format!("event: {event}\n");
        for line in data.lines() {
            frame.push_str("data: ");
            frame.push_str(line);
            frame.push('\n');
        }
        frame.push('\n');
        // 没有页面连着时发送会失败，这是正常情况。
        let _ = self.tx.send(Arc::new(frame));
    }
}

fn response(status: &str, content_type: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// 页面脚本：收到 `reload` 就刷新页面；收到 `build-error` 就把错误盖在页面上，下次刷新时清掉。
fn reload_script(addr: SocketAddr) -> String {
    format!(
        "(function () {{\n\
         \x20 var source = new EventSource('http://{addr}/__dev__/events');\n\
         \x20 source.addEventListener('reload', function () {{ location.reload(); }});\n\
         \x20 source.addEventListener('build-error', function (event) {{\n\
         \x20   var box = document.getElementById('silent-boot-build-error');\n\
         \x20   if (!box) {{\n\
         \x20     box = document.createElement('pre');\n\
         \x20     box.id = 'silent-boot-build-error';\n\
         \x20     box.style.cssText = 'position:fixed;inset:0;z-index:2147483647;margin:0;padding:16px;' +\n\
         \x20       'overflow:auto;background:#1e1e1e;color:#f48771;font:13px/1.5 monospace;white-space:pre-wrap;';\n\
         \x20     document.body.appendChild(box);\n\
         \x20   }}\n\
         \x20   box.textContent = event.data;\n\
         \x20 }});\n\
         \x20 source.addEventListener('reload', function () {{\n\
         \x20   var box = document.getElementById('silent-boot-build-error');\n\
         \x20   if (box) box.remove();\n\
         \x20 }});\n\
         }})();\n"
    )
}

async fn serve(stream: TcpStream, tx: broadcast::Sender<Arc<String>>) {
    let addr = match stream.local_addr() {
        Ok(addr) => addr,
        Err(_) => return,
    };
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // 只看请求行，其余请求头读完就丢。
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await.is_err() {
        return;
    }
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if header == "\r\n" || header == "\n" {
                    break;
                }
            }
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_owned();

    if path.ends_with("/reload.js") {
        let body = reload_script(addr);
        let _ = writer
            .write_all(&response("200 OK", "application/javascript", &body))
            .await;
        return;
    }
    if !path.ends_with("/events") {
        let _ = writer
            .write_all(&response(
                "404 Not Found",
                "text/plain; charset=utf-8",
                "not found",
            ))
            .await;
        return;
    }

    if writer.write_all(SSE_HEADERS.as_bytes()).await.is_err() {
        return;
    }
    let _ = writer.write_all(b": connected\nretry: 1000\n\n").await;
    let mut rx = tx.subscribe();
    loop {
        let frame = match rx.recv().await {
            Ok(frame) => frame,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        if writer.write_all(frame.as_bytes()).await.is_err() {
            break;
        }
    }
}
