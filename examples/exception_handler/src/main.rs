use serde::{Deserialize, Serialize};
use silent::prelude::*;

#[derive(Deserialize, Serialize, Debug)]
struct Exception {
    code: u16,
    msg: String,
}

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").get(|mut req| async move { req.params_parse::<Exception>() });
    Server::new()
        .set_exception_handler(|e| async move {
            Exception {
                code: e.status_code().as_u16(),
                msg: e.to_string(),
            }
        })
        .bind_route(route)
        .run();
}