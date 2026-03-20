# Silent TestClient 集成测试

当用户需要为 Silent 应用编写集成测试时，使用此 Skill。

## 基本用法

```rust
#[cfg(test)]
mod tests {
    use silent::prelude::*;
    use silent::testing::TestClient;

    fn setup_route() -> Route {
        Route::new_root()
            .append(Route::new("hello").get(|_req: Request| async { Ok("hello") }))
            .append(Route::new("users")
                .get(|_req: Request| async {
                    Ok(Response::json(&serde_json::json!([
                        {"id": 1, "name": "Alice"},
                    ])))
                })
                .post(|mut req: Request| async move {
                    let body: serde_json::Value = req.json_parse().await?;
                    Ok(Response::json(&body))
                })
            )
    }

    #[tokio::test]
    async fn test_get() {
        let route = setup_route();
        let resp = TestClient::get(&route, "/hello").send().await;

        resp.assert_status(200);
        resp.assert_body_contains("hello");
    }

    #[tokio::test]
    async fn test_json_post() {
        let route = setup_route();
        let resp = TestClient::post(&route, "/users")
            .json(&serde_json::json!({"name": "Bob"}))
            .send()
            .await;

        resp.assert_status(200);
        let body: serde_json::Value = resp.json().unwrap();
        assert_eq!(body["name"], "Bob");
    }
}
```

## TestClient API

### 创建请求

```rust
// HTTP 方法
TestClient::get(&route, "/path")
TestClient::post(&route, "/path")
TestClient::put(&route, "/path")
TestClient::delete(&route, "/path")
TestClient::patch(&route, "/path")

// 自定义方法
TestClient::request(&route, Method::OPTIONS, "/path")
```

### TestRequest 构建器

```rust
TestClient::post(&route, "/users")
    .header("Authorization", "Bearer token123")   // 添加请求头
    .header("X-Custom", "value")
    .json(&serde_json::json!({"name": "Alice"}))  // JSON 请求体
    .send()
    .await;

TestClient::post(&route, "/form")
    .form(&[("name", "Alice"), ("age", "30")])     // 表单请求体
    .send()
    .await;

TestClient::post(&route, "/text")
    .text("plain text body")                       // 文本请求体
    .send()
    .await;

TestClient::post(&route, "/raw")
    .body(b"raw bytes".to_vec())                   // 原始字节请求体
    .send()
    .await;
```

### TestResponse 读取

```rust
let resp = TestClient::get(&route, "/api").send().await;

// 状态码
let status: StatusCode = resp.status();

// 响应头
let headers: &HeaderMap = resp.headers();

// 响应体
let bytes: &[u8] = resp.bytes();
let text: String = resp.text();
let json: MyStruct = resp.json().unwrap();
```

### TestResponse 断言

```rust
let resp = TestClient::get(&route, "/api").send().await;

// 链式断言
resp.assert_status(200);
resp.assert_header("content-type", "application/json");
resp.assert_body_contains("success");

// 组合使用
resp.assert_status(200)
    .assert_header("x-request-id", "abc")
    .assert_body_contains("data");
```

## 测试场景示例

### 测试中间件

```rust
#[tokio::test]
async fn test_with_middleware() {
    let route = Route::new_root()
        .hook(Logger::new())
        .append(Route::new("hello").get(|_req: Request| async { Ok("ok") }));

    let resp = TestClient::get(&route, "/hello").send().await;
    resp.assert_status(200);
}
```

### 测试 404

```rust
#[tokio::test]
async fn test_not_found() {
    let route = Route::new_root()
        .append(Route::new("hello").get(|_req: Request| async { Ok("ok") }));

    let resp = TestClient::get(&route, "/nonexistent").send().await;
    resp.assert_status(404);
}
```

### 测试认证流程

```rust
#[tokio::test]
async fn test_auth_flow() {
    let route = setup_auth_route();

    // 未认证访问 — 应返回 401
    let resp = TestClient::get(&route, "/protected").send().await;
    resp.assert_status(401);

    // 登录获取 token
    let resp = TestClient::post(&route, "/login")
        .json(&serde_json::json!({"username": "admin", "password": "123"}))
        .send()
        .await;
    resp.assert_status(200);
    let body: serde_json::Value = resp.json().unwrap();
    let token = body["token"].as_str().unwrap();

    // 带 token 访问
    let resp = TestClient::get(&route, "/protected")
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await;
    resp.assert_status(200);
}
```

### 测试 CRUD

```rust
#[tokio::test]
async fn test_crud() {
    let route = setup_route();

    // Create
    let resp = TestClient::post(&route, "/users")
        .json(&serde_json::json!({"name": "Alice"}))
        .send().await;
    resp.assert_status(201);

    // Read
    let resp = TestClient::get(&route, "/users/1").send().await;
    resp.assert_status(200);

    // Update
    let resp = TestClient::put(&route, "/users/1")
        .json(&serde_json::json!({"name": "Bob"}))
        .send().await;
    resp.assert_status(200);

    // Delete
    let resp = TestClient::delete(&route, "/users/1").send().await;
    resp.assert_status(200);
}
```
