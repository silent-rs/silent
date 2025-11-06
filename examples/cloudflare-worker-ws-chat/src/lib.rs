#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
use async_lock::RwLock;
#[cfg(target_arch = "wasm32")]
use futures::StreamExt;
#[cfg(target_arch = "wasm32")]
use once_cell::sync::Lazy;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_arch = "wasm32")]
use worker::{Context, Env, Request, Response, Result, WebSocket, WebSocketPair, WebsocketEvent};

#[cfg(target_arch = "wasm32")]
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
#[cfg(target_arch = "wasm32")]
static ONLINE_USERS: Lazy<RwLock<HashMap<usize, WebSocket>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[cfg(target_arch = "wasm32")]
#[worker::event(fetch)]
pub async fn main(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let path = req.path();
    if path == "/" || path.is_empty() {
        return Response::from_html(INDEX_HTML);
    }

    if path == "/chat" {
        return handle_ws_upgrade(req).await;
    }

    Response::error("not found", 404)
}

#[cfg(target_arch = "wasm32")]
async fn handle_ws_upgrade(_req: Request) -> Result<Response> {
    // 1) 创建 WebSocketPair 并接受服务端连接
    let pair = WebSocketPair::new()?;
    let server = pair.server;
    server.accept()?;

    // 2) 注册到全局在线用户表，并为该连接分配 id
    let my_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);
    ONLINE_USERS.write().await.insert(my_id, server.clone());

    // 3) 启动事件循环：接收消息并广播给其他用户
    wasm_bindgen_futures::spawn_local(async move {
        // 向客户端发送欢迎消息
        let _ = server.send_with_str(format!("Hello User#{my_id}"));

        // 事件流
        let mut events = match server.events() {
            Ok(es) => es,
            Err(e) => {
                worker::console_error!("ws events error: {:?}", e);
                ONLINE_USERS.write().await.remove(&my_id);
                return;
            }
        };

        while let Some(evt) = events.next().await {
            match evt {
                Ok(WebsocketEvent::Message(msg)) => {
                    if let Some(text) = msg.text() {
                        // 广播消息给其他在线用户
                        let message = format!("<User#{}>: {}", my_id, text);
                        let users = ONLINE_USERS.read().await.clone();
                        for (uid, ws) in users.iter() {
                            if *uid != my_id {
                                let _ = ws.send_with_str(&message);
                            }
                        }
                    }
                }
                Ok(WebsocketEvent::Close(_)) => {
                    // 连接关闭，移除用户
                    ONLINE_USERS.write().await.remove(&my_id);
                    break;
                }
                Err(e) => {
                    worker::console_error!("ws event error: {:?}", e);
                    ONLINE_USERS.write().await.remove(&my_id);
                    break;
                }
            }
        }
    });

    // 4) 返回客户端端的 WebSocket 以完成升级
    Response::from_websocket(pair.client)
}

#[cfg(target_arch = "wasm32")]
const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>WS Chat on Cloudflare Worker</title>
    <style>
      body { font-family: system-ui, -apple-system, Segoe UI, Roboto, Ubuntu; }
      #chat { height: 280px; overflow: auto; background: #f6f8fa; padding: 8px; }
      #line { display: flex; gap: 8px; margin-top: 8px; }
      #text { flex: 1; }
    </style>
  </head>
  <body>
    <h2>WS Chat (Cloudflare Worker)</h2>
    <div id="chat"><p><em>Connecting...</em></p></div>
    <div id="line">
      <input type="text" id="text" />
      <button id="send">Send</button>
    </div>
    <script>
      const chat = document.getElementById('chat');
      const input = document.getElementById('text');
      const btn = document.getElementById('send');
      const ws = new WebSocket(`ws://${location.host}/chat`);

      function push(line) {
        const p = document.createElement('p');
        p.innerText = line;
        chat.appendChild(p);
        chat.scrollTop = chat.scrollHeight;
      }

      ws.onopen = () => { chat.innerHTML = '<p><em>Connected!</em></p>'; };
      ws.onmessage = (ev) => push(ev.data);
      ws.onclose = () => push('[disconnected]');

      btn.onclick = () => {
        const msg = input.value.trim();
        if (!msg) return;
        ws.send(msg);
        input.value = '';
        push(`<You>: ${msg}`);
      };

      input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') btn.onclick();
      });
    </script>
  </body>
</html>"#;
