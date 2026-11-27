//! 接口测试：用 `TestClient` 直接调路由，不启动服务。数据库是一个临时的 SQLite 文件，
//! 启动时自动执行迁移。
//!
//! 覆盖成功、参数校验失败、没找到三条路径，以及运维端点和包络的形状。

use {{ crate }}::app;
use serde_json::json;
use silent::StatusCode;
use silent::testing::TestClient;
use silent_boot::config::{AppConfig, AutoApply, DatabaseConfig, MigrateConfig};

async fn test_app(dir: &std::path::Path) -> silent_boot::app::App {
    let config = AppConfig {
        database: DatabaseConfig {
            url: format!("sqlite://{}/data/app.db", dir.display()),
            migrate: MigrateConfig {
                auto_apply: AutoApply::Always,
                ..MigrateConfig::default()
            },
            ..DatabaseConfig::default()
        },
        ..AppConfig::default()
    };
    app::builder(config).build().await.expect("build app")
}

async fn get(app: &silent_boot::app::App, path: &str) -> silent::testing::TestResponse {
    TestClient::get(path).send(app.router()).await
}

#[tokio::test]
async fn article_crud_goes_through_the_envelope() {
    let dir = tempfile::tempdir().expect("temp dir");
    let app = test_app(dir.path()).await;

    // 空列表。
    let res = get(&app, "/api/articles").await;
    assert_eq!(res.status(), StatusCode::OK);
    let json: serde_json::Value = res.json().await;
    assert_eq!(json["code"], 200);
    assert_eq!(json["data"]["total"], 0);
    assert_eq!(json["data"]["page"], 1);
    assert_eq!(json["data"]["pageSize"], 20);

    // 新建：201，审计字段由 service 写入。
    let res = TestClient::post("/api/articles")
        .json(&json!({ "title": "第一篇文章", "content": "正文" }))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "{}", res.text().await);
    let created: serde_json::Value = res.json().await;
    assert_eq!(created["code"], 201);
    let id = created["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(created["data"]["title"], "第一篇文章");
    assert!(created["data"]["createdAt"].is_string());

    // 详情、列表过滤。
    let res = get(&app, &format!("/api/articles/{id}")).await;
    assert_eq!(
        res.json::<serde_json::Value>().await["data"]["id"],
        id.as_str()
    );
    let res = get(&app, "/api/articles?keyword=第一").await;
    assert_eq!(res.json::<serde_json::Value>().await["data"]["total"], 1);
    let res = get(&app, "/api/articles?keyword=没有").await;
    assert_eq!(res.json::<serde_json::Value>().await["data"]["total"], 0);

    // 修改：没传的字段不动。
    let res = TestClient::put(&format!("/api/articles/{id}"))
        .json(&json!({ "status": 1 }))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value = res.json().await;
    assert_eq!(updated["data"]["status"], 1);
    assert_eq!(updated["data"]["title"], "第一篇文章");

    // 删除是软删除：列表和详情都找不到了。
    let res = TestClient::delete(&format!("/api/articles/{id}"))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.json::<serde_json::Value>().await["data"],
        serde_json::Value::Null
    );
    let res = get(&app, &format!("/api/articles/{id}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_input_is_a_422_with_field_errors() {
    let dir = tempfile::tempdir().expect("temp dir");
    let app = test_app(dir.path()).await;

    let res = TestClient::post("/api/articles")
        .json(&json!({ "title": "" }))
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json: serde_json::Value = res.json().await;
    assert_eq!(json["code"], 42_200);
    assert_eq!(json["message"], "参数校验失败");
    assert_eq!(json["data"]["title"][0]["code"], "length");
    assert!(!json["traceId"].as_str().unwrap().is_empty());

    // 请求体不是 JSON：400。
    let res = TestClient::post("/api/articles")
        .header("content-type", "text/plain")
        .text("not json")
        .send(app.router())
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 分页越界：422。
    let res = get(&app, "/api/articles?page_size=101").await;
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn unknown_resources_and_routes_are_404_envelopes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let app = test_app(dir.path()).await;

    let res = get(&app, "/api/articles/no-such-id").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let json: serde_json::Value = res.json().await;
    assert_eq!(json["code"], 40_400);
    assert_eq!(json["message"], "文章不存在");

    let res = get(&app, "/api/nope").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(res.json::<serde_json::Value>().await["code"], 40_400);
}

#[tokio::test]
async fn system_routes_and_docs_are_mounted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let app = test_app(dir.path()).await;

    for path in ["/healthz", "/readyz"] {
        let res = get(&app, path).await;
        assert_eq!(res.status(), StatusCode::OK, "{path}");
    }
    let res = get(&app, "/swagger/openapi.json").await;
    assert_eq!(res.status(), StatusCode::OK);
    let doc: serde_json::Value = res.json().await;
    assert!(doc["paths"]["/api/articles"]["get"].is_object());

    // 前端页面和它的兜底。
    let res = get(&app, "/").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.text().await.contains("<div id=\"app\">"));
    let res = get(&app, "/articles/123").await;
    assert_eq!(res.status(), StatusCode::OK);
}
