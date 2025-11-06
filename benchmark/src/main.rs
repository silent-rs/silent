use serde::{Deserialize, Serialize};
use silent::prelude::*;
use std::env;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    // 供 C 场景使用的 1KiB 静态内容与其 ETag
    blob: Arc<Vec<u8>>,
    etag: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
struct QueryB {
    q1: Option<String>,
    q2: Option<String>,
    q3: Option<i64>,
    q4: Option<bool>,
    q5: Option<String>,
}

fn build_route_a() -> Route {
    Route::new("").get(|_req: Request| async move { Ok("hello world\n") })
}

fn build_route_b() -> Route {
    // 路由: /b/<a:str>/<b:int>/<c:str>?q1=...&q2=...&q3=...&q4=...&q5=...
    Route::new("b/<a:str>/<b:int>/<c:str>").get(|mut req: Request| async move {
        let a: String = req.get_path_params("a")?;
        let b: i64 = req.get_path_params("b")?;
        let c: String = req.get_path_params("c")?;
        let q: QueryB = req.params_parse()?;

        #[derive(Serialize)]
        struct Resp<'a> {
            a: String,
            b: i64,
            c: String,
            q: QueryB,
            ok: bool,
            msg: &'a str,
        }

        let payload = Resp {
            a,
            b,
            c,
            q,
            ok: true,
            msg: "ok",
        };
        Ok(Response::json(&payload))
    })
}

fn build_route_c(state: AppState) -> Route {
    Route::new("static").get(move |req: Request| {
        let state = state.clone();
        async move {
            // If-None-Match 处理
            if let Some(im) = req
                .headers()
                .get("if-none-match")
                .and_then(|v| v.to_str().ok())
            {
                if im == state.etag {
                    let mut res = Response::empty();
                    res.set_status(StatusCode::NOT_MODIFIED);
                    res.set_header(
                        header::HeaderName::from_static("etag"),
                        header::HeaderValue::from_str(&state.etag).unwrap(),
                    );
                    return Ok(res);
                }
            }
            let mut res = Response::empty();
            res.set_header(
                header::HeaderName::from_static("content-type"),
                header::HeaderValue::from_static("application/octet-stream"),
            );
            res.set_header(
                header::HeaderName::from_static("etag"),
                header::HeaderValue::from_str(&state.etag).unwrap(),
            );
            res.set_body(full((*state.blob).clone()));
            Ok(res)
        }
    })
}

fn make_state() -> AppState {
    let blob = vec![b'x'; 1024];
    // 简单 ETag（非强校验，仅用于基准）
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    blob.hash(&mut hasher);
    let etag = format!("\"{:x}\"", hasher.finish());
    AppState {
        blob: Arc::new(blob),
        etag,
    }
}

#[tokio::main]
pub async fn main() {
    logger::fmt().with_max_level(Level::WARN).init();
    let scenario = env::var("SCENARIO").unwrap_or_else(|_| "A".into());
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let route = match scenario.as_str() {
        // 场景 A：GET / 返回 12B 文本
        "A" => build_route_a(),
        // 场景 B：解析 3 个路径参数 + 5 个查询参数 + 回 JSON
        "B" => build_route_b(),
        // 场景 C：1KiB 静态文件（带 ETag / If-None-Match） -> GET /static
        "C" => build_route_c(make_state()),
        other => {
            eprintln!("Unknown SCENARIO: {other}, fallback to A");
            build_route_a()
        }
    };

    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", port).parse().expect("invalid PORT");
    Server::new().bind(addr).serve(route).await;
}
