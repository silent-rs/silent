//! M0 验证：Web 链路，对应 `docs/design.md` §16 的 V1、V3、V4。
//!
//! - V1：`with_state` 注入连接、提取器 handler、统一错误响应、TestClient 走通一次 CRUD。
//! - V3：自定义提取器可以作为元组的一部分参与 handler。
//! - V4：SPA 兜底路由不抢 API 和静态资源；静态资源缺失时仍是 404。
mod common;

use std::path::Path;

use common::app::AppMigrator;
use common::entity::article;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use silent::extractor::{FromRequest, Json, Query, State};
use silent::headers::ContentType;
use silent::middlewares::ExceptionHandler;
use silent::prelude::*;
use silent::testing::TestClient;
use silent::{MiddleWareHandler, Next, Request};
use silent_boot::db::DbRegistry;
use silent_boot::db::migrate::{FrameworkMigrator, MigrationRunner};

#[derive(Clone)]
struct AppState {
    db: DbRegistry,
}

#[derive(Debug, Deserialize)]
struct CreateArticle {
    title: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArticleQuery {
    keyword: Option<String>,
}

/// 测试用的"当前用户"：由 `FakeAuth` 写进请求扩展，再由提取器取出来。
#[derive(Clone, Debug)]
struct CurrentUser {
    id: String,
}

fn db_error(e: sea_orm::DbErr) -> SilentError {
    SilentError::business_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn create(
    (State(app), Json(input)): (State<AppState>, Json<CreateArticle>),
) -> Result<Response> {
    let now = chrono::Local::now().naive_local();
    let model = article::ActiveModel {
        id: Set(scru128::new_string()),
        title: Set(input.title),
        content: Set(input.content),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(app.db.db())
    .await
    .map_err(db_error)?;
    Ok(Response::json(&model).with_status(StatusCode::CREATED))
}

async fn list(
    (State(app), Query(query)): (State<AppState>, Query<ArticleQuery>),
) -> Result<Response> {
    let mut select = article::Entity::find();
    if let Some(keyword) = query.keyword {
        select = select.filter(article::Column::Title.contains(keyword));
    }
    let items = select.all(app.db.db()).await.map_err(db_error)?;
    Ok(Response::json(&items))
}

async fn me(
    (State(_), user, Query(_query)): (State<AppState>, CurrentUser, Query<ArticleQuery>),
) -> Result<Response> {
    Ok(Response::json(&serde_json::json!({ "id": user.id })))
}

#[async_trait::async_trait]
impl FromRequest for CurrentUser {
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        req.extensions()
            .get::<CurrentUser>()
            .cloned()
            .ok_or_else(|| SilentError::business_error(StatusCode::UNAUTHORIZED, "login required"))
    }
}

/// 从请求头取用户，写进请求扩展。
struct FakeAuth;

#[async_trait::async_trait]
impl MiddleWareHandler for FakeAuth {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        let user = req
            .headers()
            .get("x-user")
            .and_then(|v| v.to_str().ok())
            .map(|id| CurrentUser { id: id.to_owned() });
        if let Some(user) = user {
            req.extensions_mut().insert(user);
        }
        next.call(req).await
    }
}

/// 失败响应统一成 `{ code, message }`。
fn with_error_envelope(route: Route) -> Route {
    route.hook(ExceptionHandler::new(
        |result: Result<Response>, _state: silent::State| async move {
            match result {
                Ok(res) => Ok(res),
                Err(e) => Ok(Response::json(&serde_json::json!({
                    "code": e.status().as_u16(),
                    "message": e.message(),
                }))
                .with_status(e.status())),
            }
        },
    ))
}

/// SPA 兜底：文件存在就返回文件，否则返回 `index.html`。
///
/// 这里只是验证路由优先级，内容类型一律按 HTML 返回；正式实现要按扩展名判断。
fn spa_fallback(
    root: String,
) -> impl Fn(Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response>> + Send>>
{
    move |req: Request| {
        let root = root.clone();
        Box::pin(async move {
            let path = req.uri().path().trim_start_matches('/');
            let candidate = Path::new(&root).join(path);
            let file = if !path.contains("..") && candidate.is_file() {
                candidate
            } else {
                Path::new(&root).join("index.html")
            };
            let content = std::fs::read_to_string(file).map_err(|e| {
                SilentError::business_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            })?;
            let mut res = Response::empty();
            res.set_body(content.into());
            res.set_typed_header(ContentType::html());
            Ok(res)
        })
    }
}

/// 建好表的数据库。迁移里的样例数据已经写入两行 article。
///
/// 返回的 `TempDir` 必须在测试期间一直持有：目录被删掉以后，SQLite 还能读，但写会报
/// `attempt to write a readonly database`。
async fn setup() -> (tempfile::TempDir, DbRegistry) {
    let (dir, db) = common::temp_db().await;
    MigrationRunner::new("web")
        .up(&db, &FrameworkMigrator, &AppMigrator, None)
        .await
        .expect("migrate");
    (dir, DbRegistry::new(db))
}

#[tokio::test]
async fn v1_minimal_chain_crud_through_test_client() {
    let (_web_dir, db) = setup().await;
    let route = with_error_envelope(
        Route::new_root()
            .append(Route::new("api").append(Route::new("articles").get(list).post(create))),
    )
    .with_state(AppState { db });

    // 列表：迁移里的样例数据有两行。
    let resp = TestClient::get("/api/articles").send(&route).await;
    let listed: serde_json::Value = resp.assert_status(StatusCode::OK).json().await;
    assert_eq!(listed.as_array().map(Vec::len), Some(2));

    // 新增：201，返回新建的记录。
    let body = serde_json::json!({ "title": "hello", "content": "first" });
    let resp = TestClient::post("/api/articles")
        .json(&body)
        .send(&route)
        .await;
    let status = resp.status();
    let text = resp.text().await;
    assert_eq!(status, StatusCode::CREATED, "body: {text}");
    let created: serde_json::Value = serde_json::from_str(&text).expect("created is json");
    assert_eq!(created["title"], "hello");
    assert!(created["created_at"].is_string());

    // 查询参数：按关键字过滤。
    let resp = TestClient::get("/api/articles?keyword=hello")
        .send(&route)
        .await;
    let listed: serde_json::Value = resp.assert_status(StatusCode::OK).json().await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));

    // 失败路径：Content-Type 不对，返回 400。
    // 实测：提取器拒绝的响应是纯文本的消息，不经过 ExceptionHandler，所以这里不是统一包络。
    let resp = TestClient::post("/api/articles")
        .text("not json")
        .send(&route)
        .await;
    let status = resp.status();
    let text = resp.text().await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {text}");
    assert_eq!(text, "content-type is error");

    // 失败路径：Content-Type 正确，但请求体不是合法的 JSON，返回 422。
    // 实测：响应体是 serde 的原始错误信息（纯文本），同样不经过 ExceptionHandler。
    let resp = TestClient::post("/api/articles")
        .header("content-type", "application/json")
        .body("not json")
        .send(&route)
        .await;
    let status = resp.status();
    let text = resp.text().await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {text}");
    assert!(text.contains("expected ident"), "body: {text}");

    // 失败路径：未匹配的路由返回 404。这一类错误会经过 ExceptionHandler，是统一包络。
    let resp = TestClient::get("/api/nope").send(&route).await;
    let body: serde_json::Value = resp.assert_status(StatusCode::NOT_FOUND).json().await;
    assert_eq!(body["code"], 404);
    assert_eq!(body["message"], "not found");
}

#[tokio::test]
async fn v3_custom_extractor_joins_extractor_tuple() {
    let (_web_dir, db) = setup().await;
    let route = with_error_envelope(
        Route::new_root()
            .hook(FakeAuth)
            .append(Route::new("api").append(Route::new("me").get(me))),
    )
    .with_state(AppState { db });

    // 没有登录信息：提取器拒绝，返回 401。
    // 实测：拒绝响应是纯文本的消息，不经过 ExceptionHandler，所以这里不是统一包络。
    let resp = TestClient::get("/api/me").send(&route).await;
    let status = resp.status();
    let text = resp.text().await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body: {text}");
    assert_eq!(text, "login required");

    let resp = TestClient::get("/api/me")
        .header("x-user", "u-1")
        .send(&route)
        .await;
    let body: serde_json::Value = resp.assert_status(StatusCode::OK).json().await;
    assert_eq!(body["id"], "u-1");
}

#[tokio::test]
async fn v4_spa_fallback_leaves_api_and_static_assets_alone() {
    let web = tempfile::tempdir().expect("temp dir");
    let assets = web.path().join("assets");
    std::fs::create_dir_all(&assets).expect("assets dir");
    std::fs::write(web.path().join("index.html"), "<h1>app</h1>").expect("index.html");
    std::fs::write(assets.join("app.js"), "console.log(1)").expect("app.js");
    let root = web.path().to_str().expect("utf-8 path").to_owned();
    let assets_dir = assets.to_str().expect("utf-8 path").to_owned();

    let (_web_dir, db) = setup().await;
    let route = with_error_envelope(
        Route::new_root()
            .append(Route::new("api").append(Route::new("articles").get(list)))
            .append(Route::new("assets").with_static(&assets_dir))
            .append(Route::new("<path:**>").get(spa_fallback(root))),
    )
    .with_state(AppState { db });

    // API 不受兜底影响。
    TestClient::get("/api/articles")
        .send(&route)
        .await
        .assert_status(StatusCode::OK);

    // 前端路由回退到 index.html。
    let resp = TestClient::get("/articles/123").send(&route).await;
    let body = resp.assert_status(StatusCode::OK).text().await;
    assert_eq!(body, "<h1>app</h1>");

    // 已有的静态资源由静态处理器返回。
    let resp = TestClient::get("/assets/app.js").send(&route).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(resp.text().await, "console.log(1)");

    // 缺失的静态资源仍是 404，不会被兜底成 index.html。
    TestClient::get("/assets/missing.js")
        .send(&route)
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
