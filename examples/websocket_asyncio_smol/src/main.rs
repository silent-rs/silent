use silent::prelude::*;
use silent::service::transport::AsyncIoTransport;

fn main() {
    logger::fmt().init();
    smol::block_on(async move {
        let route = Route::new("").get(show_form).append(
            Route::new("ws").ws(
                None,
                WebSocketHandler::new()
                    .on_connect(|_, _| async { Ok(()) })
                    .on_send(|msg, _| async { Ok(msg) })
                    .on_receive(|_, _| async { Ok(()) })
                    .on_close(|_| async {}),
            ),
        );
        Server::new()
            .with_transport(AsyncIoTransport::new())
            .bind("127.0.0.1:8000".parse().unwrap())
            .serve(route)
            .await;
    });
}

async fn show_form(_req: Request) -> Result<Response> {
    Ok(Response::html(
        r#"<!DOCTYPE html>
<html>
    <head>
        <title>WS</title>
    </head>
    <body>
        <h1>WS</h1>
        <div id="status">
            <p><em>Connecting...</em></p>
        </div>
        <script>
            const status = document.getElementById('status');
            const ws = new WebSocket(`ws://${location.host}/ws`);
            ws.onopen = function() {
                status.innerHTML = '<p><em>Connected!</em></p>';
            };
        </script>
    </body>
</html>
"#,
    ))
}
