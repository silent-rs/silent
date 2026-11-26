//! M1 验证：启动器装配出来的应用，用 `TestClient` 和真实 TCP 连接各走一遍。
//!
//! - 统一响应与错误：成功包络带 `traceId`，handler 的 `Err`、提取器拒绝、panic、纯文本错误响应都是包络；
//! - 带校验的提取器：JSON 不合法是 400，字段不满足规则是 422，`data` 里带字段错误；
//! - 分页：`PageQuery` 的默认值和上限，`PageResult` 的形状；
//! - 运维端点：`/healthz` 永远 200，`/readyz` 在迁移没执行完时是 503；
//! - 静态资源与 SPA 兜底不抢 `api` 前缀；`base_path` 生效；CORS 和压缩按配置装配；
//! - OpenAPI：`/swagger` 能打开，`openapi.json` 里有接口和包络的类型；
//! - `auto_apply = "always"` 在启动时建库。
//!
//! 这里的 dto / service / api 就是脚手架生成的示例模块的写法。
//!
//! 示例模块带着 OpenAPI 的标注，所以整个文件只在启用 `openapi` feature 时编译。

#![cfg(feature = "openapi")]

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use common::app::AppMigrator;
use common::entity::article;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use silent::testing::{TestClient, TestResponse};
use silent::{Request, Response, StatusCode};
use silent_boot::config::{
    AppConfig, AutoApply, DatabaseConfig, MigrateConfig, OpenApiConfig, ServerConfig,
};
use silent_boot::db::migrate::FrameworkMigrator;
use silent_boot::prelude::*;
use silent_boot::web::assets::MemoryAssets;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use validator::Validate;

// ---------- dto ----------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateArticleDto {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ArticleQueryDto {
    /// 标题包含的关键字。
    pub keyword: Option<String>,
    /// 按状态过滤：0 草稿，1 已发布。
    pub status: Option<i32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArticleDto {
    pub id: String,
    pub title: String,
    pub content: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

impl From<article::Model> for ArticleDto {
    fn from(model: article::Model) -> Self {
        Self {
            id: model.id,
            title: model.title,
            content: model.content,
            created_at: model.created_at,
        }
    }
}

// ---------- service ----------

pub struct ArticleService;

impl ArticleService {
    pub async fn page(
        db: &sea_orm::DatabaseConnection,
        page: &PageQuery,
        q: &ArticleQueryDto,
    ) -> AppResult<PageResult<ArticleDto>> {
        let mut select = article::Entity::find().filter(article::Column::DeletedAt.is_null());
        if let Some(keyword) = q.keyword.as_deref().filter(|k| !k.is_empty()) {
            select = select.filter(article::Column::Title.contains(keyword));
        }
        if let Some(status) = q.status {
            select = select.filter(article::Column::Status.eq(status));
        }
        let select = select.order_by_desc(article::Column::CreatedAt);
        Ok(paginate(db, select, page, ArticleDto::from).await?)
    }

    pub async fn create(
        db: &sea_orm::DatabaseConnection,
        input: &CreateArticleDto,
    ) -> AppResult<ArticleDto> {
        let now = chrono::Local::now().naive_local();
        let model = article::ActiveModel {
            id: Set(scru128::new_string()),
            title: Set(input.title.clone()),
            content: Set(input.content.clone()),
            status: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok(model.into())
    }

    pub async fn get(db: &sea_orm::DatabaseConnection, id: &str) -> AppResult<ArticleDto> {
        article::Entity::find_by_id(id)
            .filter(article::Column::DeletedAt.is_null())
            .one(db)
            .await?
            .map(ArticleDto::from)
            .ok_or_else(|| AppError::not_found("文章"))
    }
}

// ---------- api ----------

#[derive(utoipa::OpenApi)]
#[openapi(
    info(title = "demo", version = "0.1.0"),
    paths(page, create, detail),
    components(schemas(
        ArticleDto,
        CreateArticleDto,
        Envelope<ArticleDto>,
        Envelope<PageResult<ArticleDto>>
    ))
)]
struct ApiDoc;

#[utoipa::path(
    get,
    path = "/api/articles",
    params(ArticleQueryDto, PageQuery),
    responses((status = 200, body = Envelope<PageResult<ArticleDto>>))
)]
async fn page(
    (State(app), page, ValidatedQuery(query)): (
        State<AppState>,
        Page,
        ValidatedQuery<ArticleQueryDto>,
    ),
) -> Result<Envelope<PageResult<ArticleDto>>> {
    Ok(Envelope::ok(
        ArticleService::page(app.db.db(), &page.0, &query).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/articles",
    request_body = CreateArticleDto,
    responses((status = 201, body = Envelope<ArticleDto>), (status = 422))
)]
async fn create(
    (State(app), ValidatedJson(input)): (State<AppState>, ValidatedJson<CreateArticleDto>),
) -> Result<Created<ArticleDto>> {
    Ok(Created(Envelope::created(
        ArticleService::create(app.db.db(), &input).await?,
    )))
}

#[utoipa::path(
    get,
    path = "/api/articles/{id}",
    params(("id" = String, Path, description = "文章 ID")),
    responses((status = 200, body = Envelope<ArticleDto>), (status = 404))
)]
async fn detail(
    (State(app), Path(id)): (State<AppState>, Path<String>),
) -> Result<Envelope<ArticleDto>> {
    Ok(Envelope::ok(ArticleService::get(app.db.db(), &id).await?))
}

/// 故意出错的几个 handler，只给测试用。
async fn boom(_req: Request) -> Result<Response> {
    panic!("kaboom");
}

async fn forbidden(_app: State<AppState>) -> Result<Envelope<()>> {
    Err(AppError::Forbidden("article:delete".to_owned()).into())
}

async fn plain_text_error(_req: Request) -> Result<Response> {
    Ok(Response::text("something is off").with_status(StatusCode::BAD_REQUEST))
}

async fn plain_ok(_req: Request) -> Result<Response> {
    Ok(Response::text("just text"))
}

pub fn routes() -> Route {
    Route::new("api")
        .append(
            Route::new("articles")
                .get(page)
                .post(create)
                .append(Route::new("<id>").get(detail)),
        )
        .append(Route::new("boom").get(boom))
        .append(Route::new("forbidden").get(forbidden))
        .append(Route::new("plain-error").get(plain_text_error))
        .append(Route::new("plain-ok").get(plain_ok))
}

// ---------- 夹具 ----------

fn assets() -> MemoryAssets {
    MemoryAssets(
        [(
            "index.html".to_owned(),
            b"<html><body><div id=app></div></body></html>".to_vec(),
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>(),
    )
}

fn config(dir: &std::path::Path) -> AppConfig {
    AppConfig {
        database: DatabaseConfig {
            url: format!("sqlite://{}/data/app.db", dir.display()),
            migrate: MigrateConfig {
                auto_apply: AutoApply::Always,
                ..MigrateConfig::default()
            },
            ..DatabaseConfig::default()
        },
        server: ServerConfig {
            listen: "127.0.0.1:0".to_owned(),
            ..ServerConfig::default()
        },
        openapi: OpenApiConfig {
            enabled: Some(true),
            ..OpenApiConfig::default()
        },
        ..AppConfig::default()
    }
}

async fn build(
    dir: &std::path::Path,
    tune: impl FnOnce(AppBuilder) -> AppBuilder,
) -> silent_boot::app::App {
    let builder = App::with_config(config(dir))
        .migrator(AppMigrator)
        .route(routes())
        .openapi(ApiDoc::openapi())
        .assets(assets());
    tune(builder).build().await.expect("build app")
}

async fn get(app: &silent_boot::app::App, path: &str) -> TestResponse {
    TestClient::get(path).send(app.router()).await
}

async fn body(res: TestResponse) -> serde_json::Value {
    res.json().await
}

// ---------- 测试 ----------

#[tokio::test]
async fn v18_startup_creates_the_database_and_sample_data_is_listed() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!dir.path().join("data/app.db").exists());
    let app = build(dir.path(), |b| b).await;
    assert!(dir.path().join("data/app.db").exists());

    let res = get(&app, "/healthz").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body(res).await, serde_json::json!({ "status": "ok" }));

    let res = get(&app, "/readyz").await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", res.text().await);

    let res = get(&app, "/api/articles").await;
    assert_eq!(res.status(), StatusCode::OK);
    let header_trace = res
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let json = body(res).await;
    // 迁移里写了两行样例数据。
    assert_eq!(json["code"], 200);
    assert_eq!(json["message"], "success");
    assert_eq!(json["data"]["page"], 1);
    assert_eq!(json["data"]["pageSize"], 20);
    assert_eq!(json["data"]["total"], 2);
    assert_eq!(json["data"]["items"].as_array().unwrap().len(), 2);
    assert!(
        json["data"]["items"][0]["createdAt"]
            .as_str()
            .is_some_and(|s| s.contains('T')),
        "{json}"
    );
    let trace_id = json["traceId"].as_str().unwrap().to_owned();
    assert_eq!(trace_id.len(), 25, "traceId is a scru128 id");
    assert_eq!(header_trace, trace_id);
}

#[tokio::test]
async fn v19_crud_with_envelopes_validation_and_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| b).await;

    // 新建：201 的包络。
    let res = TestClient::post("/api/articles")
        .json(&serde_json::json!({ "title": "新文章", "content": "正文" }))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = body(res).await;
    assert_eq!(created["code"], 201);
    let id = created["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(created["data"]["title"], "新文章");

    // 详情。
    let res = get(&app, &format!("/api/articles/{id}")).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body(res).await["data"]["id"], id.as_str());

    // 列表：过滤和分页。
    let res = get(&app, "/api/articles?keyword=%E6%96%B0&page=1&page_size=1").await;
    let json = body(res).await;
    assert_eq!(json["data"]["total"], 1);
    assert_eq!(json["data"]["items"].as_array().unwrap().len(), 1);

    // 校验失败：422，data 里是字段错误。
    let res = TestClient::post("/api/articles")
        .json(&serde_json::json!({ "title": "" }))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body(res).await;
    assert_eq!(json["code"], 42_200);
    assert_eq!(json["message"], "参数校验失败");
    assert_eq!(json["data"]["title"][0]["code"], "length");
    assert!(!json["traceId"].as_str().unwrap().is_empty());

    // 缺字段：反序列化失败是 400。
    let res = TestClient::post("/api/articles")
        .json(&serde_json::json!({ "name": "没有标题" }))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body(res).await["code"], 40_000);

    // 请求体不是 JSON：Silent 的 Json 提取器给出纯文本的 400，被改写成包络。
    let res = TestClient::post("/api/articles")
        .header("content-type", "text/plain")
        .text("not json")
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = body(res).await;
    assert_eq!(json["code"], 40_000);
    assert_eq!(json["message"], "请求错误：content-type is error");

    // 分页参数不是数字、越界：422，data 里是字段错误。
    let res = get(&app, "/api/articles?page=abc").await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body(res).await["data"]["page"][0]["code"], "parse");

    let res = get(&app, "/api/articles?page=0&page_size=101").await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body(res).await;
    assert_eq!(json["data"]["page"][0]["code"], "range");
    assert_eq!(json["data"]["page_size"][0]["code"], "range");

    // 查询参数的类型不对：400 的包络。
    let res = get(&app, "/api/articles?status=abc").await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body(res).await["code"], 40_000);

    // 不存在的资源：404，业务码 40400，说明来自 service。
    let res = get(&app, "/api/articles/no-such-id").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let json = body(res).await;
    assert_eq!(json["code"], 40_400);
    assert_eq!(json["message"], "文章不存在");

    // 没注册的接口：404 的包络，不回退到 index.html。
    let res = get(&app, "/api/nope").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(body(res).await["code"], 40_400);

    // 方法不对：405。
    let res = TestClient::delete("/api/articles").send(app.router()).await;
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body(res).await["code"], 40_500);
}

#[tokio::test]
async fn v20_errors_from_handlers_all_become_envelopes() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| b).await;

    // AppError 带着业务码穿过 SilentError 还原回来。
    let res = get(&app, "/api/forbidden").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let json = body(res).await;
    assert_eq!(json["code"], 40_301);
    assert_eq!(json["message"], "无权限：article:delete");
    assert_eq!(json["data"], serde_json::Value::Null);

    // handler 自己返回的纯文本错误响应。
    let res = get(&app, "/api/plain-error").await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body(res).await["message"], "something is off");

    // handler 里的 panic 不断连接，返回 500 的包络，不暴露细节。
    let res = get(&app, "/api/boom").await;
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body(res).await;
    assert_eq!(json["code"], 50_000);
    assert_eq!(json["message"], "服务器内部错误");
    assert!(!json.to_string().contains("kaboom"));

    // 不是包络形状的成功响应原样返回。
    let res = get(&app, "/api/plain-ok").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await, "just text");
}

#[tokio::test]
async fn v21_trace_id_is_generated_echoed_or_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| b).await;

    let first = get(&app, "/healthz").await;
    let second = get(&app, "/healthz").await;
    let a = first
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let b = second
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_ne!(a, b);

    let res = TestClient::get("/healthz")
        .header("x-request-id", "edge-42")
        .send(app.router())
        .await;
    assert_eq!(res.headers().get("x-request-id").unwrap(), "edge-42");

    // 带非法字符的追踪号不采信，换一个新的。
    let res = TestClient::get("/healthz")
        .header("x-request-id", "bad id\n")
        .send(app.router())
        .await;
    assert_ne!(res.headers().get("x-request-id").unwrap(), "bad id\n");

    // 500 的响应也带追踪号，方便对着日志查。
    let res = get(&app, "/api/boom").await;
    assert!(res.headers().get("x-request-id").is_some());
}

#[tokio::test]
async fn v22_readiness_reports_pending_migrations_until_they_run() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| {
        let mut cfg = config(dir.path());
        cfg.database.migrate.auto_apply = AutoApply::Never;
        b.config(cfg)
    })
    .await;
    let res = get(&app, "/readyz").await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json = body(res).await;
    assert_eq!(json["code"], 50_300);
    assert_eq!(
        json["data"]["reasons"],
        serde_json::json!([
            "framework migrations pending: 1",
            "app migrations pending: 2"
        ])
    );
    assert_eq!(get(&app, "/healthz").await.status(), StatusCode::OK);

    // 执行迁移之后就绪。
    silent_boot::db::migrate::MigrationRunner::default()
        .up(app.state().db.db(), &FrameworkMigrator, &AppMigrator, None)
        .await
        .unwrap();
    let res = get(&app, "/readyz").await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", res.text().await);
}

#[tokio::test]
async fn v23_static_assets_spa_fallback_and_base_path() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| b).await;

    let res = get(&app, "/").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.text().await.contains("<div id=app>"));
    let res = get(&app, "/articles/123").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.text().await.contains("<div id=app>"));
    let res = get(&app, "/missing.png").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 部署在子路径下。
    let app = build(dir.path(), |b| {
        let mut config = config(dir.path());
        config.app.base_path = "/admin".to_owned();
        b.config(config)
    })
    .await;
    // 系统路由和业务路由都在子路径下。
    let res = get(&app, "/admin/healthz").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await, r#"{"status":"ok"}"#);
    let res = get(&app, "/admin/readyz").await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = get(&app, "/admin/api/articles").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body(res).await["code"], 200);
    let res = get(&app, "/admin/api/nope").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(body(res).await["code"], 40_400);
    let res = get(&app, "/admin/swagger/openapi.json").await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = get(&app, "/admin/assets/none.js").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = get(&app, "/admin/whatever/route").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.text().await.contains("<div id=app>"));

    // 子路径之外什么都没有：既没有系统路由，也没有前端兜底。
    for path in ["/", "/healthz", "/api/articles", "/whatever/route"] {
        let res = get(&app, path).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn v24_cors_compression_and_openapi_follow_config() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| {
        b.config(AppConfig {
            cors: silent_boot::config::CorsConfig {
                allowed_origins: vec!["http://localhost:5173".to_owned()],
                ..Default::default()
            },
            ..config(dir.path())
        })
    })
    .await;

    let res = TestClient::get("/api/articles")
        .header("origin", "http://localhost:5173")
        .send(app.router())
        .await;
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "http://localhost:5173"
    );
    let res = TestClient::get("/api/articles")
        .header("origin", "http://evil.example")
        .send(app.router())
        .await;
    assert!(res.headers().get("access-control-allow-origin").is_none());

    let res = TestClient::get("/api/articles")
        .header("accept-encoding", "gzip")
        .send(app.router())
        .await;
    let encoding = res
        .headers()
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert_eq!(encoding, "gzip", "large JSON responses are compressed");

    // Swagger UI 和文档。
    let res = get(&app, "/swagger/").await;
    assert_eq!(res.status(), StatusCode::OK);
    let html = res.text().await;
    assert!(html.contains("SwaggerUIBundle"), "{html}");
    assert!(html.contains("/swagger/openapi.json"), "{html}");
    let res = get(&app, "/swagger/openapi.json").await;
    assert_eq!(res.status(), StatusCode::OK);
    let doc: serde_json::Value = body(res).await;
    assert_eq!(doc["openapi"], "3.1.0");
    assert_eq!(doc["info"]["title"], "demo");
    let paths = doc["paths"].as_object().unwrap();
    assert!(paths["/api/articles"]["get"].is_object(), "{doc}");
    assert!(paths["/api/articles"]["post"].is_object());
    assert_eq!(
        doc["paths"]["/api/articles"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/Envelope_PageResult_ArticleDto"
    );
    let schemas = doc["components"]["schemas"].as_object().unwrap();
    assert!(schemas["ArticleDto"].is_object());
    assert!(schemas["Envelope_PageResult_ArticleDto"]["properties"]["traceId"].is_object());
    assert!(
        schemas["Envelope_PageResult_ArticleDto"]["properties"]["data"]["properties"]["pageSize"]
            .is_object()
    );
    eprintln!(
        "parameters: {}",
        doc["paths"]["/api/articles"]["get"]["parameters"]
    );
    // 线上环境默认不提供文档，且 api 前缀下的未知路径仍是 404。
    let app = build(dir.path(), |b| {
        let mut cfg = config(dir.path());
        cfg.app.env = silent_boot::config::Env::Production;
        cfg.openapi.enabled = None;
        b.config(cfg)
    })
    .await;
    let res = get(&app, "/swagger/").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn v25_app_serves_tcp_and_stops_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| b).await;
    let listener = app.bind().await.unwrap();
    let addr = listener.local_addr().unwrap();
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
    assert_ne!(addr.port(), 0, "the OS picked a free port");

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(app.serve(listener, async move {
        let _ = stopped.await;
    }));

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    assert!(raw.starts_with("HTTP/1.1 200"), "{raw}");
    assert!(raw.contains("x-request-id"), "{raw}");
    assert!(raw.contains(r#"{"status":"ok"}"#), "{raw}");

    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("server stops")
        .unwrap()
        .unwrap();

    // 监听已经关掉。
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}

#[tokio::test]
async fn v26_bind_failure_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = taken.local_addr().unwrap().to_string();
    let app = build(dir.path(), |b| {
        let mut cfg = config(dir.path());
        cfg.server.listen = addr.clone();
        b.config(cfg)
    })
    .await;
    let err = app.bind().await.expect_err("port is taken");
    let text = err.to_string();
    assert!(text.contains(&addr), "{text}");
    assert!(text.contains("监听"), "{text}");
}

#[tokio::test]
async fn v27_state_is_shared_and_migrators_are_reachable() {
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), |b| b).await;
    let migrators = app.state().migrators();
    let pending = migrators
        .app
        .pending_read_only(app.state().db.db())
        .await
        .unwrap();
    assert!(pending.is_empty());
    assert_eq!(app.config().app.name, "app");
    assert_eq!(app.config().database.url, app.state().config.database.url);
    assert_eq!(app.state().env, silent_boot::config::Env::Development);
}
