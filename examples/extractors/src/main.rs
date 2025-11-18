use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use silent::extractor::{
    Configs, Extension, Form, FromRequest, Json, Method, Path, Query, TypedHeader, Uri, Version,
    handler_from_extractor,
};
use silent::headers::UserAgent;
use silent::prelude::{Route, Server, logger};
use silent::{Handler, MiddleWareHandler, Next, Request, Response, Result};
use tracing::{Level, info};

#[derive(Deserialize)]
struct Page {
    page: u32,
    size: u32,
}

async fn user_detail(
    req: Request,
    (Path(id), Query(p)): (Path<i64>, Query<Page>),
) -> Result<String> {
    info!("req: {:?}", req.uri());
    Ok(format!("id={id}, page={}, size={}", p.page, p.size))
}

#[derive(Deserialize, Serialize)]
struct CreateUser {
    name: String,
    age: u32,
}

async fn create_user(Json(input): Json<CreateUser>) -> Result<String> {
    Ok(format!("created: {} ({})", input.name, input.age))
}

// Path 单值
async fn ex_path_id(Path(id): Path<i64>) -> Result<String> {
    Ok(format!("path.id={id}"))
}

// Path 结构体
#[derive(Deserialize)]
struct UserPath {
    id: i64,
    name: String,
}

async fn ex_path_struct(Path(up): Path<UserPath>) -> Result<String> {
    Ok(format!("user.id={}, user.name={}", up.id, up.name))
}

// Query
async fn ex_query_old(Query(p): Query<Page>) -> Result<String> {
    Ok(format!("query.page={}, query.size={}", p.page, p.size))
}

// Multi Query structs simultaneously
#[derive(Deserialize)]
struct Search {
    keyword: Option<String>,
}

async fn ex_multi_query((Query(s), Query(p)): (Query<Search>, Query<Page>)) -> Result<String> {
    Ok(format!(
        "keyword={:?}, page={}, size={}",
        s.keyword, p.page, p.size
    ))
}

// Form
async fn ex_form(Form(input): Form<CreateUser>) -> Result<String> {
    Ok(format!("form: {} ({})", input.name, input.age))
}

// TypedHeader
async fn ex_typed_header(TypedHeader(ua): TypedHeader<UserAgent>) -> Result<String> {
    Ok(format!("ua={}", ua.as_str()))
}

// Method / Uri / Version
async fn ex_method(Method(m): Method) -> Result<String> {
    Ok(format!("method={:?}", m))
}

async fn ex_uri(Uri(u): Uri) -> Result<String> {
    Ok(format!("uri={}", u))
}

async fn ex_version(Version(v): Version) -> Result<String> {
    Ok(format!("version={:?}", v))
}

// Extension
#[derive(Clone)]
struct MyExt(&'static str);
async fn ex_extension(Extension(MyExt(v)): Extension<MyExt>) -> Result<String> {
    Ok(format!("ext={v}"))
}

// Configs
#[derive(Clone)]
struct MyCfg(u32);
async fn ex_configs(Configs(MyCfg(v)): Configs<MyCfg>) -> Result<String> {
    Ok(format!("cfg={v}"))
}

// ===== Query 参数结构体 =====
#[derive(Debug, Deserialize)]
struct NameAgeQuery {
    name: Option<String>,
    age: Option<u32>,
}

// Path 参数结构体
#[derive(Deserialize)]
struct IdPath {
    #[serde(rename = "id")]
    single_id: i64,
}

// Header 结构体
#[derive(Deserialize)]
#[allow(dead_code)]
struct HeadersQuery {
    #[serde(rename = "user-agent")]
    user_agent: Option<String>,
    #[serde(rename = "content-type")]
    content_type: Option<String>,
}

// Cookie 结构体
#[derive(Deserialize)]
#[allow(dead_code)]
struct CookiesQuery {
    session: Option<String>,
    user: Option<String>,
}

// 类型转换查询参数
#[derive(Deserialize)]
struct TypeConversionQuery {
    count: Option<i32>,
    active: Option<bool>,
    price: Option<f64>,
    size: Option<u64>,
}

// ===== 使用结构体萃取器的示例 =====

// Query - 使用结构体萃取器
async fn ex_query(Query(params): Query<NameAgeQuery>) -> Result<String> {
    let name = params.name.unwrap_or_default();
    let age = params.age.unwrap_or(0);
    Ok(format!("query: name={}, age={}", name, age))
}

// Path - 使用结构体萃取器
async fn ex_path(Path(params): Path<IdPath>) -> Result<String> {
    Ok(format!(
        "path: id from Path={}, single_id={}",
        params.single_id, params.single_id
    ))
}

// Headers - 使用 TypedHeader
async fn ex_headers(TypedHeader(ua): TypedHeader<UserAgent>) -> Result<String> {
    Ok(format!("headers: user-agent={}", ua.as_str()))
}

// Cookies - 使用辅助函数从 HeaderMap 提取
async fn ex_cookies(req: Request) -> Result<String> {
    let cookies = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let mut session = String::new();
    let mut user = String::new();

    for part in cookies.split(';') {
        let mut kv = part.trim().split('=');
        if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
            if k == "session" {
                session = v.to_string();
            } else if k == "user" {
                user = v.to_string();
            }
        }
    }

    Ok(format!("cookies: session={}, user={}", session, user))
}

// Config - 使用结构体萃取器
async fn ex_config(Configs(cfg): Configs<MyCfg>) -> Result<String> {
    Ok(format!("config: cfg={}", cfg.0))
}

// 组合使用多个萃取器
async fn ex_combined(args: (Query<NameAgeQuery>, Configs<MyCfg>)) -> Result<String> {
    let (Query(params), Configs(cfg)) = args;
    let name = params.name.unwrap_or("guest".to_string());
    Ok(format!(
        "combined: name={}, age={}, cfg={}",
        name,
        params.age.unwrap_or(0),
        cfg.0
    ))
}

// 类型转换示例
async fn ex_type_conversion(Query(params): Query<TypeConversionQuery>) -> Result<String> {
    let count = params.count.unwrap_or(0);
    let active = params.active.unwrap_or(false);
    let price = params.price.unwrap_or(0.0);
    let size = params.size.unwrap_or(0);

    Ok(format!(
        "type conversion: count={}, active={}, price={}, size={}",
        count, active, price, size
    ))
}

// 错误处理示例
async fn ex_error_handling(mut req: Request) -> Result<String> {
    // 使用 Query 萃取器，缺少参数时返回错误
    match Query::<NameAgeQuery>::from_request(&mut req).await {
        Ok(Query(params)) => Ok(format!("成功获取参数: {:?}", params)),
        Err(_) => Ok("错误：缺少必需参数".to_string()),
    }
}

// Option 与 Result 萃取器
async fn ex_option_id(opt: Option<Path<i64>>) -> Result<String> {
    Ok(match opt {
        Some(Path(id)) => format!("some({id})"),
        None => "none".into(),
    })
}

async fn ex_result_json(res: std::result::Result<Json<CreateUser>, Response>) -> Result<String> {
    match res {
        Ok(Json(v)) => Ok(format!("ok: {} ({})", v.name, v.age)),
        Err(e) => Ok(format!("bad_request: {:?}", e)),
    }
}

// Tuple + Request 兼容（保留原示例）

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();

    // 顶层中间件：注入 Extension 与 Configs 以方便演示
    #[derive(Clone)]
    struct Inject;
    #[async_trait]
    impl MiddleWareHandler for Inject {
        async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
            req.extensions_mut().insert(MyExt("hello"));
            req.configs_mut().insert(MyCfg(7));
            next.call(req).await
        }
    }

    let route = Route::new("api")
        .hook(Inject)
        // path
        .append(Route::new("path/<id:int>").get(ex_path_id))
        .append(Route::new("path_struct/<id:i64>/<name>").get(ex_path_struct))
        // query
        .append(Route::new("query").get(ex_query_old))
        .append(Route::new("multi_query").get(ex_multi_query))
        // json
        .append(Route::new("json").post(create_user))
        // form（使用显式适配器避免重载歧义）
        .append(
            Route::new("form").post(handler_from_extractor::<Form<CreateUser>, _, _, _>(ex_form)),
        )
        // typed header
        .append(Route::new("typed_header").get(ex_typed_header))
        // method/uri/version
        .append(Route::new("method").get(ex_method))
        .append(Route::new("uri").get(ex_uri))
        .append(Route::new("version").get(ex_version))
        // extension/configs
        .append(Route::new("extension").get(ex_extension))
        .append(Route::new("configs").get(ex_configs))
        // ===== 使用结构体萃取器的示例路由 =====
        .append(Route::new("struct/query").get(ex_query))
        .append(Route::new("struct/path/<id:int>").get(ex_path))
        .append(Route::new("struct/headers").get(ex_headers))
        .append(Route::new("struct/cookies").get(ex_cookies))
        .append(Route::new("struct/config").get(ex_config))
        .append(Route::new("struct/combined").get(handler_from_extractor::<
            (Query<NameAgeQuery>, Configs<MyCfg>),
            _,
            _,
            _,
        >(ex_combined)))
        .append(Route::new("struct/type_conversion").get(ex_type_conversion))
        .append(Route::new("struct/error").get(ex_error_handling))
        // option/result extractors
        .append(Route::new("opt_id").get(ex_option_id))
        .append(Route::new("opt_id/<id:int>").get(ex_option_id))
        .append(Route::new("result_json").post(ex_result_json))
        // tuple + request （沿用原示例）
        .append(Route::new("tuple/<id:int>").get(user_detail));
    Server::new().run(route);
}
