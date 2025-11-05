use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use silent::extractor::{
    config_param, cookie_param, header_param, path_param, query_param, Configs, Extension, Form,
    Json, Method, Path, Query, TypedHeader, Uri, Version, handler_from_extractor,
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
async fn ex_query(Query(p): Query<Page>) -> Result<String> {
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

// ===== 新增：单个字段萃取器示例 =====

// QueryParam - 单个查询参数
async fn ex_query_param(mut req: Request) -> Result<String> {
    let name = query_param::<String>(&mut req, "name")
        .await
        .unwrap_or_default();
    let age = query_param::<u32>(&mut req, "age").await.unwrap_or(0);
    Ok(format!("query_param: name={}, age={}", name, age))
}

// PathParam - 单个路径参数
async fn ex_path_param(mut req: Request, Path(id): Path<i64>) -> Result<String> {
    let single_id = path_param::<i64>(&mut req, "id").await.unwrap_or_default();
    Ok(format!(
        "path_param: id from Path={}, single_id={}",
        id, single_id
    ))
}

// HeaderParam - 单个请求头
async fn ex_header_param(mut req: Request) -> Result<String> {
    let user_agent = header_param::<String>(&mut req, "user-agent")
        .await
        .unwrap_or_default();
    let content_type = header_param::<String>(&mut req, "content-type")
        .await
        .unwrap_or_default();
    Ok(format!(
        "header_param: user-agent={}, content-type={}",
        user_agent, content_type
    ))
}

// CookieParam - 单个 Cookie
async fn ex_cookie_param(mut req: Request) -> Result<String> {
    let session = cookie_param::<String>(&mut req, "session")
        .await
        .unwrap_or_default();
    let user = cookie_param::<String>(&mut req, "user")
        .await
        .unwrap_or_default();
    Ok(format!("cookie_param: session={}, user={}", session, user))
}

// ConfigParam - 单个配置
async fn ex_config_param(mut req: Request) -> Result<String> {
    let cfg = config_param::<MyCfg>(&mut req).await.unwrap_or(MyCfg(0));
    Ok(format!("config_param: cfg={}", cfg.0))
}

// 组合使用多个单个字段萃取器
async fn ex_combined_extractors(mut req: Request) -> Result<String> {
    let name = query_param::<String>(&mut req, "name")
        .await
        .unwrap_or("guest".to_string());
    let user_agent = header_param::<String>(&mut req, "user-agent")
        .await
        .unwrap_or_default();
    let session = cookie_param::<String>(&mut req, "session")
        .await
        .unwrap_or_default();
    let cfg = config_param::<MyCfg>(&mut req).await.unwrap_or(MyCfg(0));

    Ok(format!(
        "combined: name={}, ua={}, session={}, cfg={}",
        name, user_agent, session, cfg.0
    ))
}

// 类型转换示例
async fn ex_type_conversion(mut req: Request) -> Result<String> {
    let count = query_param::<i32>(&mut req, "count").await.unwrap_or(0);
    let active = query_param::<bool>(&mut req, "active")
        .await
        .unwrap_or(false);
    let price = query_param::<f64>(&mut req, "price").await.unwrap_or(0.0);
    let size = query_param::<u64>(&mut req, "size").await.unwrap_or(0);

    Ok(format!(
        "type conversion: count={}, active={}, price={}, size={}",
        count, active, price, size
    ))
}

// 错误处理示例
async fn ex_error_handling(mut req: Request) -> Result<String> {
    // 演示单个字段萃取器的错误处理
    let result = query_param::<String>(&mut req, "required_param").await;

    match result {
        Ok(value) => Ok(format!("成功获取参数: {}", value)),
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
        .append(Route::new("query").get(ex_query))
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
        // ===== 新增：单个字段萃取器示例路由 =====
        .append(Route::new("single/query").get(ex_query_param))
        .append(Route::new("single/path/<id:int>").get(ex_path_param))
        .append(Route::new("single/header").get(ex_header_param))
        .append(Route::new("single/cookie").get(ex_cookie_param))
        .append(Route::new("single/config").get(ex_config_param))
        .append(Route::new("single/combined").get(ex_combined_extractors))
        .append(Route::new("single/type_conversion").get(ex_type_conversion))
        .append(Route::new("single/error").get(ex_error_handling))
        // option/result extractors
        .append(Route::new("opt_id").get(ex_option_id))
        .append(Route::new("opt_id/<id:int>").get(ex_option_id))
        .append(Route::new("result_json").post(ex_result_json))
        // tuple + request （沿用原示例）
        .append(Route::new("tuple/<id:int>").get(user_detail));
    Server::new().run(route);
}
