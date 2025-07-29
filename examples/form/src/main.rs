use serde::{Deserialize, Serialize};
use silent::prelude::*;

fn main() {
    logger::fmt().init();
    let route = Route::new("").get(show_form).post(accept_form);
    Server::new().run(route);
}

#[derive(Deserialize, Serialize, Debug)]
#[allow(dead_code)]
struct Input {
    name: String,
    email: String,
}

async fn accept_form(mut req: Request) -> Result<Input> {
    // 使用 form_parse 来处理表单数据（支持 application/x-www-form-urlencoded 和 multipart/form-data）
    req.form_parse().await
}

async fn show_form(_req: Request) -> Result<Response> {
    Ok(Response::html(
        r#"
        <!doctype html>
        <html>
            <head></head>
            <body>
                <form action="/" method="post">
                    <label for="name">
                        Enter your name:
                        <input type="text" name="name">
                    </label>

                    <label>
                        Enter your email:
                        <input type="text" name="email">
                    </label>

                    <input type="submit" value="Subscribe!">
                </form>
            </body>
        </html>
        "#,
    ))
}
