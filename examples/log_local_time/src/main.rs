use silent::prelude::*;

fn main() {
    logger::fmt()
        .with_timer(logger::fmt::time::ChronoLocal::rfc_3339())
        .with_max_level(Level::INFO)
        .init();
    let route = Route::new("").get(|_req: Request| async { Ok("hello world") });
    Server::new().run(route);
}
