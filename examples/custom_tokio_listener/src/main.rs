use async_trait::async_trait;
use silent::prelude::*;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").handler(
        Method::GET,
        Arc::new(CustomHandler {
            count: AtomicUsize::new(0),
        }),
    );
    let listener: Listener = TcpListener::bind("127.0.0.1:8000").await.unwrap().into();
    Server::new().listen(listener).serve(route).await;
}

struct CustomHandler {
    count: AtomicUsize,
}

#[async_trait]
impl Handler for CustomHandler {
    async fn call(&self, _req: Request) -> Result<Response> {
        let html = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <title>custom handler</title>
        </head>
        <body>
            <h1>custom handler</h1>
            <p>count: "#
            .to_string()
            + &self
                .count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .to_string()
            + r#"</p>
        </body>
        </html>"#;
        Ok(html.into())
    }
}
