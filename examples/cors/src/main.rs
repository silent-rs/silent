use silent::middlewares::{Cors, CorsType};
use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("")
        .hook(
            Cors::new()
                .origin(CorsType::Any)
                .methods(CorsType::Any)
                .headers(CorsType::Any)
                .credentials(true),
        )
        .get(|mut req: Request| async move {
            let ok = req.params().get("ok").is_some();
            if ok {
                Err(SilentError::business_error(
                    StatusCode::BAD_REQUEST,
                    "bad request".to_string(),
                ))
            } else {
                Ok("hello world")
            }
        })
        .post(|_req: Request| async { Ok("hello world") })
        .put(|_req: Request| async { Ok("hello world") })
        .patch(|_req: Request| async { Ok("hello world") })
        .options(|_req: Request| async { Ok("hello world") })
        .delete(|_req: Request| async { Ok("hello world") });
    Server::new().run(route);
}
