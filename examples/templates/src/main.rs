use serde::Serialize;
use silent::prelude::*;

#[derive(Serialize)]
struct Temp {
    name: String,
}

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let mut route = Route::new("")
        .get(|_req: Request| async {
            let temp = Temp {
                name: "templates".to_string(),
            };
            Ok(TemplateResponse::from(("index.html".to_string(), temp)))
        })
        .route();
    route.set_template_dir("examples/templates/templates/**/*");
    Server::new().run(route);
}
