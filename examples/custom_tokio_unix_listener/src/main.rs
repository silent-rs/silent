//! Run with
//!
//! ```not_rust
//! cargo run -p example-custom_tokio_unix_listener
//! ```
#[cfg(unix)]
#[tokio::main]
async fn main() {
    unix::server().await;
}

#[cfg(not(unix))]
fn main() {
    println!("This example requires unix")
}

#[cfg(unix)]
mod unix {
    use http_body_util::BodyExt;
    use hyper_util::rt::TokioIo;
    use silent::prelude::*;
    use silent::prelude::{HandlerAppend, Level, Route, Server, logger};
    use std::time::Duration;
    use tokio::net::{UnixListener, UnixStream};

    pub async fn server() {
        logger::fmt().with_max_level(Level::INFO).init();
        let listener_path = "./examples/custom_tokio_unix_listener/custom_handler.sock";

        tokio::spawn(async move {
            let route = Route::new("").get(handler);
            let listener: Listener = UnixListener::bind(listener_path).unwrap().into();

            Server::new().listen(listener).serve(route).await;
            // Server::new().bind_unix(listener_path).serve(route).await;
        });

        tokio::time::sleep(Duration::from_secs(1)).await;

        let stream = TokioIo::new(UnixStream::connect(listener_path).await.unwrap());
        let (mut sender, conn) = hyper::client::conn::http1::handshake(stream).await.unwrap();
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        let request = Request::empty();

        let response = sender.send_request(request.into_http()).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(body, "Hello, World!");

        let _ = tokio::fs::remove_file(listener_path).await;
    }

    async fn handler(req: Request) -> Result<&'static str> {
        println!("new connection from `{:?}`", req.remote());

        Ok("Hello, World!")
    }
}
