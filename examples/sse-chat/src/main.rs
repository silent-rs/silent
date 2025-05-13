// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
//
// port from https://github.com/seanmonstar/warp/blob/master/examples/sse_chat.rs

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures_util::StreamExt;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use silent::prelude::*;

type Users = Mutex<HashMap<usize, mpsc::UnboundedSender<Message>>>;

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
static ONLINE_USERS: Lazy<Users> = Lazy::new(Users::default);

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();

    let route = Route::new("").get(index).append(
        Route::new("chat")
            .get(user_connected)
            .append(Route::new("<id:int>").post(chat_send)),
    );

    Server::new()
        .bind("0.0.0.0:8001".parse().unwrap())
        .run(route);
}

#[derive(Debug)]
enum Message {
    UserId(usize),
    Reply(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Msg {
    msg: String,
}

async fn chat_send(mut req: Request) -> Result<Response> {
    let my_id = req.get_path_params::<i32>("id")?;
    let msg = req.json_parse::<Msg>().await?;
    info!("chat_send: my_id: {}, msg: {:?}", my_id, msg);
    user_message(my_id as usize, msg.msg.as_str());
    Ok(Response::empty())
}

async fn user_connected(_req: Request) -> Result<Response> {
    // Use a counter to assign a new unique ID for this user.
    let my_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    info!("new chat user: {}", my_id);

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the event source...
    let (tx, rx) = mpsc::unbounded_channel();
    let rx = UnboundedReceiverStream::new(rx);

    tx.send(Message::UserId(my_id))
        // rx is right above, so this cannot fail
        .unwrap();

    // Save the sender in our list of connected users.
    ONLINE_USERS.lock().insert(my_id, tx);

    // Convert messages into Server-Sent Events and returns resulting stream.
    let stream = rx.map(|msg| match msg {
        Message::UserId(my_id) => {
            warn!("user {} disconnected", my_id);
            Ok(SSEEvent::default().event("user").data(my_id.to_string()))
        }
        Message::Reply(reply) => {
            warn!("sse_reply: {}", reply);
            Ok(SSEEvent::default().data(reply))
        }
    });
    sse_reply(stream)
}

fn user_message(my_id: usize, msg: &str) {
    let new_msg = format!("<User#{my_id}>: {msg}");

    // New message from this user, send it to everyone else (except same uid)...
    //
    // We use `retain` instead of a for loop so that we can reap any user that
    // appears to have disconnected.
    ONLINE_USERS.lock().retain(|uid, tx| {
        if my_id == *uid {
            // don't send to same user, but do retain
            true
        } else {
            // If not `is_ok`, the SSE stream is gone, and so don't retain
            tx.send(Message::Reply(new_msg.clone())).is_ok()
        }
    });
}

async fn index<'a>(_req: Request) -> Result<&'a str> {
    Ok(INDEX_HTML)
}

static INDEX_HTML: &str = r#"
<!DOCTYPE html>
<html>
    <head>
        <title>SSE Chat</title>
    </head>
    <body>
        <h1>SSE chat</h1>
        <div id="chat">
            <p><em>Connecting...</em></p>
        </div>
        <input type="text" id="text" />
        <button type="button" id="send">Send</button>
        <script type="text/javascript">
        var uri = 'http://' + location.host + '/chat';
        var sse = new EventSource(uri);
        function message(data) {
            var line = document.createElement('p');
            line.innerText = data;
            chat.appendChild(line);
        }
        sse.onopen = function() {
            chat.innerHTML = "<p><em>Connected!</em></p>";
        }
        var user_id;
        sse.addEventListener("user", function(msg) {
            user_id = msg.data;
        });
        sse.onmessage = function(msg) {
            message(msg.data);
        };
        send.onclick = function() {
            var msg = text.value;
            var xhr = new XMLHttpRequest();
            xhr.open("POST", uri + '/' + user_id, true);
            xhr.setRequestHeader('Content-Type','application/json');
            xhr.send(`{"msg":"${msg}"}`);
            text.value = '';
            message('<You>: ' + msg);
        };
        </script>
    </body>
</html>
"#;
