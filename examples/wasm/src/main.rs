use silent::prelude::*;

#[tokio::main]
async fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").get(|_req: Request| async { Ok("hello world") });
    Server::new().serve(route).await;
}
