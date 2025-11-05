// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
//
// port from https://github.com/seanmonstar/warp/blob/master/examples/websocket_chat.rs

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_channel as mpsc;
use async_lock::RwLock;
use once_cell::sync::Lazy;

use silent::prelude::*;

type Users = RwLock<HashMap<usize, mpsc::Sender<Message>>>;

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
static ONLINE_USERS: Lazy<Users> = Lazy::new(Users::default);

fn main() {
    logger::fmt().init();
    let route = Route::new("").get(index).append(
        Route::new("chat").ws(
            None,
            WebSocketHandler::new()
                .on_connect(on_connect)
                .on_send(on_send)
                .on_receive(on_receive)
                .on_close(on_close),
        ),
    );
    Server::new().run(route);
}

async fn on_connect(
    parts: Arc<RwLock<WebSocketParts>>,
    sender: mpsc::Sender<Message>,
) -> Result<()> {
    let mut parts = parts.write().await;
    info!("{:?}", parts);
    let my_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);
    info!("new chat user: {}", my_id);
    parts.extensions_mut().insert(my_id);
    sender
        .try_send(Message::text(format!("Hello User#{my_id}")))
        .unwrap();
    ONLINE_USERS.write().await.insert(my_id, sender);
    Ok(())
}

async fn on_send(message: Message, _parts: Arc<RwLock<WebSocketParts>>) -> Result<Message> {
    info!("on_send: {:?}", message);
    Ok(message)
}

async fn on_receive(message: Message, parts: Arc<RwLock<WebSocketParts>>) -> Result<()> {
    let parts = parts.read().await;
    let my_id = parts.extensions().get::<usize>().unwrap();
    info!("on_receive: {:?}", message);
    let msg = if let Ok(s) = message.to_str() {
        s
    } else {
        return Err(SilentError::BusinessError {
            code: StatusCode::BAD_REQUEST,
            msg: "invalid message".to_string(),
        });
    };
    let message = Message::text(format!("<User#{my_id}>: {msg}"));
    for (uid, tx) in ONLINE_USERS.read().await.iter() {
        if my_id != uid {
            let _ = tx.try_send(message.clone());
        }
    }
    Ok(())
}

async fn on_close(parts: Arc<RwLock<WebSocketParts>>) {
    let parts = parts.read().await;
    let my_id = parts.extensions().get::<usize>().unwrap();
    info!("good bye user: {my_id}");
    // Stream closed up, so remove from the user list
    ONLINE_USERS.write().await.remove(my_id);
}

async fn index(_res: Request) -> Result<Response> {
    Ok(Response::html(INDEX_HTML))
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>WS Chat</title>
    </head>
    <body>
        <h1>WS Chat</h1>
        <div id="chat">
            <p><em>Connecting...</em></p>
        </div>
        <input type="text" id="text" />
        <button type="button" id="submit">Submit</button>
        <script>
            const chat = document.getElementById('chat');
            const msg = document.getElementById('msg');
            const submit = document.getElementById('submit');
            const ws = new WebSocket(`ws://${location.host}/chat`);

            ws.onopen = function() {
                chat.innerHTML = '<p><em>Connected!</em></p>';
            };

            ws.onmessage = function(msg) {
                showMessage(msg.data);
            };

            ws.onclose = function() {
                chat.getElementsByTagName('em')[0].innerText = 'Disconnected!';
            };

            submit.onclick = function() {
                const msg = text.value;
                ws.send(msg);
                text.value = '';

                showMessage('<You>: ' + msg);
            };
            function showMessage(data) {
                const line = document.createElement('p');
                line.innerText = data;
                chat.appendChild(line);
            }
        </script>
    </body>
</html>
"#;
