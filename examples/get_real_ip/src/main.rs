use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").get(|req: Request| async move { Ok(req.remote().to_string()) });
    Server::new().run(route);
}
