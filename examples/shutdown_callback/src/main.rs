use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").get(|_req: Request| async { Ok("hello world") });
    Server::new()
        .set_shutdown_callback(|| info!("server stop graceful!"))
        .run(route);
}
