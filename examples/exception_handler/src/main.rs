use serde::{Deserialize, Serialize};

use silent::middlewares::ExceptionHandler;
use silent::prelude::*;

#[derive(Deserialize, Serialize, Debug)]
struct Exception {
    code: u16,
    msg: String,
}

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("")
        .hook(ExceptionHandler::new(|res, _| async move { res }))
        .get(|mut req: Request| async move { req.params_parse::<Exception>() })
        .route();
    Server::new().run(route);
}
