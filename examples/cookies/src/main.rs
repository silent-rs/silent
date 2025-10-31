use silent::header::{CONTENT_TYPE, HeaderValue};
use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").get(custom_response);
    Server::new().run(route);
}

async fn custom_response(_req: Request) -> Result<Response> {
    let mut res = Response::empty();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text"));
    let html = r#"
    <!DOCTYPE html>
    <html lang="en">
    <head
        <meta charset="UTF-8">
        <title>custom response</title>
    </head>
    <body>
        <h1>custom response</h1>
    </body>
    </html>"#;
    res.set_body(full(html));
    res.cookies_mut().add(
        Cookie::build(("hello", "world"))
            .max_age(CookieTime::Duration::hours(2))
            .secure(true),
    );
    Ok(res)
}
