# 集成测试指南

Silent 提供 `TestClient` 集成测试工具，可以在不启动 TCP 服务器的情况下直接调用路由，用于测试处理器、中间件和完整请求链路。

## 依赖配置

在 `Cargo.toml` 中启用 `test` feature：

```toml
[dev-dependencies]
silent = { version = "2.15", features = ["test"] }
tokio = { version = "1", features = ["macros", "rt"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## 快速上手

```rust
use silent::prelude::*;
use silent::testing::TestClient;

#[tokio::test]
async fn test_hello() {
    let app = Route::new_root()
        .append(Route::new("hello").get(|_: Request| async { Ok("Hello!") }));

    let resp = TestClient::get("/hello").send(&app).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.text().await, "Hello!");
}
```

## API 概览

### TestClient

提供 HTTP 方法的静态构建入口：

| 方法 | 说明 |
|------|------|
| `TestClient::get(path)` | 创建 GET 请求 |
| `TestClient::post(path)` | 创建 POST 请求 |
| `TestClient::put(path)` | 创建 PUT 请求 |
| `TestClient::delete(path)` | 创建 DELETE 请求 |
| `TestClient::patch(path)` | 创建 PATCH 请求 |
| `TestClient::request(method, path)` | 创建自定义方法请求 |

### TestRequest

链式构建器，设置请求参数后调用 `send()` 发送：

```rust
TestClient::post("/api/users")
    .header("Authorization", "Bearer token123")
    .json(&CreateUser { name: "Alice".into() })
    .send(&app)
    .await;
```

| 方法 | 说明 |
|------|------|
| `.header(name, value)` | 添加请求头 |
| `.json(&data)` | 设置 JSON 请求体（自动设置 Content-Type） |
| `.form(data)` | 设置表单请求体（自动设置 Content-Type） |
| `.text(data)` | 设置文本请求体（自动设置 Content-Type） |
| `.body(data)` | 设置原始字节请求体 |
| `.send(&handler)` | 发送请求并返回 `TestResponse` |

### TestResponse

封装响应结果，提供读取方法和链式断言：

**读取方法**（消耗 self）：

| 方法 | 说明 |
|------|------|
| `.status()` | 获取 HTTP 状态码 |
| `.headers()` | 获取响应头 |
| `.text().await` | 获取响应体字符串 |
| `.bytes().await` | 获取响应体字节 |
| `.json::<T>().await` | 解析响应体为 JSON |

**断言方法**（返回 self，支持链式调用）：

| 方法 | 说明 |
|------|------|
| `.assert_status(code)` | 断言状态码 |
| `.assert_header(name, value)` | 断言响应头值 |
| `.assert_header_exists(name)` | 断言响应头存在 |
| `.assert_body_contains(sub)` | 断言响应体包含子串 |
| `.assert_body_eq(expected)` | 断言响应体等于指定字符串 |
| `.assert_json(&expected)` | 断言 JSON 响应体相等 |

```rust
TestClient::get("/api/user/1")
    .send(&app)
    .await
    .assert_status(StatusCode::OK)
    .assert_header("content-type", "application/json")
    .assert_body_contains("Alice");
```

## 常见测试场景

### 测试 JSON 接口

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct User {
    id: u64,
    name: String,
}

#[tokio::test]
async fn test_create_user() {
    let app = Route::new_root().append(
        Route::new("users").post(|mut req: Request| async move {
            let user: User = req.json_parse().await?;
            Ok(Response::json(&user))
        }),
    );

    let expected = User { id: 1, name: "Alice".into() };

    TestClient::post("/users")
        .json(&expected)
        .send(&app)
        .await
        .assert_status(StatusCode::OK)
        .assert_json(&expected);
}
```

### 测试中间件

```rust
use silent::middlewares::RequestId;

#[tokio::test]
async fn test_request_id_middleware() {
    let app = Route::new_root().append(
        Route::new("api")
            .hook(RequestId::new())
            .get(|_: Request| async { Ok("ok") }),
    );

    TestClient::get("/api")
        .send(&app)
        .await
        .assert_status(StatusCode::OK)
        .assert_header_exists("x-request-id");
}
```

### 测试错误响应

```rust
#[tokio::test]
async fn test_not_found() {
    let app = Route::new_root()
        .append(Route::new("exists").get(|_: Request| async { Ok("ok") }));

    let resp = TestClient::get("/not-exists").send(&app).await;
    assert_ne!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_business_error() {
    let app = Route::new_root().append(
        Route::new("fail").get(|_: Request| async {
            Err::<Response, _>(SilentError::business_error(
                StatusCode::BAD_REQUEST,
                "invalid input".to_string(),
            ))
        }),
    );

    TestClient::get("/fail")
        .send(&app)
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_body_contains("invalid input");
}
```

### 测试表单提交

```rust
#[tokio::test]
async fn test_form_submit() {
    let app = Route::new_root().append(
        Route::new("login").post(|req: Request| async move {
            let ct = req.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            Ok(ct)
        }),
    );

    TestClient::post("/login")
        .form("username=admin&password=secret")
        .send(&app)
        .await
        .assert_status(StatusCode::OK)
        .assert_body_contains("x-www-form-urlencoded");
}
```

## 复杂场景：Token 认证全流程测试

以下示例演示从登录获取 token、携带 token 访问受保护资源、到登出注销 token 的完整认证流程测试。

### 1. 定义数据结构

```rust
use serde::{Deserialize, Serialize};
use silent::prelude::*;
use silent::testing::TestClient;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// 登录请求
#[derive(Serialize, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

/// 登录响应（包含 token）
#[derive(Serialize, Deserialize, Debug)]
struct LoginResponse {
    token: String,
    username: String,
}

/// 业务数据
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Profile {
    username: String,
    role: String,
}

/// 已认证用户（存储在 Request extensions 中）
#[derive(Clone, Debug)]
struct AuthUser {
    username: String,
    role: String,
}

/// Token 存储（模拟数据库，线程安全）
#[derive(Clone)]
struct TokenStore {
    /// token -> username 映射
    tokens: Arc<Mutex<HashSet<String>>>,
}
```

### 2. 实现认证中间件

```rust
use async_trait::async_trait;

/// JWT/Token 认证中间件
struct AuthMiddleware {
    store: TokenStore,
}

#[async_trait]
impl MiddleWareHandler for AuthMiddleware {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        // 从 Authorization 头提取 token
        let token = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                SilentError::business_error(
                    StatusCode::UNAUTHORIZED,
                    "缺少 Authorization 头".to_string(),
                )
            })?
            .to_string();

        // 验证 token 是否有效（是否在存储中）
        let is_valid = self.store.tokens.lock().unwrap().contains(&token);
        if !is_valid {
            return Err(SilentError::business_error(
                StatusCode::UNAUTHORIZED,
                "无效或已过期的 token".to_string(),
            ));
        }

        // 解析 token 中的用户信息（简化：token 格式为 "token-{username}"）
        let username = token
            .strip_prefix("token-")
            .unwrap_or("unknown")
            .to_string();

        // 将用户信息注入到 request extensions 中
        req.extensions_mut().insert(AuthUser {
            username,
            role: "user".to_string(),
        });

        // 继续处理请求
        next.call(req).await
    }
}
```

### 3. 构建应用路由

```rust
fn build_app(store: TokenStore) -> Route {
    Route::new_root()
        // 公开接口：登录（不需要认证）
        .append({
            let store = store.clone();
            Route::new("auth/login").post(move |mut req: Request| {
                let store = store.clone();
                async move {
                    let login: LoginRequest = req.json_parse().await?;

                    // 验证用户名密码（简化）
                    if login.username == "admin" && login.password == "123456" {
                        let token = format!("token-{}", login.username);
                        store.tokens.lock().unwrap().insert(token.clone());

                        Ok(Response::json(&LoginResponse {
                            token,
                            username: login.username,
                        }))
                    } else {
                        Err(SilentError::business_error(
                            StatusCode::UNAUTHORIZED,
                            "用户名或密码错误".to_string(),
                        ))
                    }
                }
            })
        })
        // 受保护接口：需要认证
        .append(
            Route::new("api")
                .hook(AuthMiddleware { store: store.clone() })
                // 获取用户资料
                .append(
                    Route::new("profile").get(|req: Request| async move {
                        let user = req.extensions().get::<AuthUser>().ok_or_else(|| {
                            SilentError::business_error(
                                StatusCode::UNAUTHORIZED,
                                "未认证".to_string(),
                            )
                        })?;
                        Ok(Response::json(&Profile {
                            username: user.username.clone(),
                            role: user.role.clone(),
                        }))
                    }),
                )
                // 登出
                .append({
                    let store = store.clone();
                    Route::new("logout").post(move |req: Request| {
                        let store = store.clone();
                        async move {
                            // 从 header 中获取 token 并移除
                            if let Some(token) = req
                                .headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.strip_prefix("Bearer "))
                            {
                                store.tokens.lock().unwrap().remove(token);
                            }
                            Ok(Response::json(&serde_json::json!({"message": "已登出"})))
                        }
                    })
                }),
        )
}
```

### 4. 编写完整流程测试

```rust
#[tokio::test]
async fn test_full_auth_flow() {
    let store = TokenStore {
        tokens: Arc::new(Mutex::new(HashSet::new())),
    };
    let app = build_app(store);

    // ========== 第一步：未登录访问受保护资源，应返回 401 ==========
    TestClient::get("/api/profile")
        .send(&app)
        .await
        .assert_status(StatusCode::UNAUTHORIZED)
        .assert_body_contains("缺少 Authorization 头");

    // ========== 第二步：使用错误密码登录，应返回 401 ==========
    TestClient::post("/auth/login")
        .json(&LoginRequest {
            username: "admin".into(),
            password: "wrong".into(),
        })
        .send(&app)
        .await
        .assert_status(StatusCode::UNAUTHORIZED)
        .assert_body_contains("用户名或密码错误");

    // ========== 第三步：使用正确密码登录，获取 token ==========
    let login_resp = TestClient::post("/auth/login")
        .json(&LoginRequest {
            username: "admin".into(),
            password: "123456".into(),
        })
        .send(&app)
        .await
        .assert_status(StatusCode::OK);

    let login_data: LoginResponse = login_resp.json().await;
    assert_eq!(login_data.username, "admin");
    let token = login_data.token;

    // ========== 第四步：携带 token 访问受保护资源 ==========
    TestClient::get("/api/profile")
        .header("Authorization", format!("Bearer {}", token))
        .send(&app)
        .await
        .assert_status(StatusCode::OK)
        .assert_json(&Profile {
            username: "admin".into(),
            role: "user".into(),
        });

    // ========== 第五步：使用无效 token 访问，应返回 401 ==========
    TestClient::get("/api/profile")
        .header("Authorization", "Bearer invalid-token")
        .send(&app)
        .await
        .assert_status(StatusCode::UNAUTHORIZED)
        .assert_body_contains("无效或已过期的 token");

    // ========== 第六步：登出 ==========
    TestClient::post("/api/logout")
        .header("Authorization", format!("Bearer {}", token))
        .send(&app)
        .await
        .assert_status(StatusCode::OK)
        .assert_body_contains("已登出");

    // ========== 第七步：登出后再次访问，token 已失效 ==========
    TestClient::get("/api/profile")
        .header("Authorization", format!("Bearer {}", token))
        .send(&app)
        .await
        .assert_status(StatusCode::UNAUTHORIZED)
        .assert_body_contains("无效或已过期的 token");
}
```

### 要点总结

1. **TestClient 直接调用 Handler**：无需启动 HTTP 服务器，测试速度极快
2. **中间件通过 `hook()` 挂载**：认证中间件仅作用于挂载的路由子树
3. **Extensions 传递状态**：中间件通过 `req.extensions_mut().insert()` 存储认证信息，handler 通过 `req.extensions().get::<T>()` 读取
4. **链式断言**：`assert_status()` → `assert_header()` → `assert_body_contains()` 可以连续调用
5. **`json().await` 反序列化**：直接将响应体解析为结构体，方便提取 token 等字段
6. **共享状态**：使用 `Arc<Mutex<T>>` 在路由和中间件间共享 token 存储

## 注意事项

- `TestClient` 不经过网络层，直接调用 `Handler::call()`，因此不会触发 TCP 连接相关行为
- 默认设置 remote addr 为 `127.0.0.1:0`，部分中间件（如限流）可能依赖此值
- `text()` / `bytes()` / `json()` 会消耗 `TestResponse`，调用后不可再次读取
- 断言方法返回 `self`，可以链式调用，但注意 `json()` 等消耗方法不能和断言链混用
