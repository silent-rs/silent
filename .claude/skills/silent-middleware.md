# Silent 自定义中间件编写

当用户需要创建自定义中间件时，使用此 Skill。

## MiddleWareHandler Trait

```rust
use async_trait::async_trait;

#[async_trait]
pub trait MiddleWareHandler: Send + Sync + 'static {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response>;
}
```

## 中间件模板

```rust
use async_trait::async_trait;
use silent::prelude::*;

#[derive(Clone)]
pub struct MyMiddleware {
    // 配置字段
}

impl MyMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl MiddleWareHandler for MyMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        // === 前置处理（请求到达处理器之前）===

        // 调用下一个中间件或处理器
        let res = next.call(req).await;

        // === 后置处理（处理器响应之后）===

        res
    }
}
```

## 常见中间件模式

### 1. 请求日志

```rust
#[derive(Clone)]
pub struct RequestLogger;

#[async_trait]
impl MiddleWareHandler for RequestLogger {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let start = std::time::Instant::now();

        let res = next.call(req).await;

        let elapsed = start.elapsed();
        match &res {
            Ok(r) => tracing::info!("{} {} {} {:?}", method, path, r.status().as_u16(), elapsed),
            Err(e) => tracing::error!("{} {} {} {:?}", method, path, e.status().as_u16(), elapsed),
        }
        res
    }
}
```

### 2. 认证校验

```rust
#[derive(Clone)]
pub struct AuthMiddleware {
    secret: String,
}

impl AuthMiddleware {
    pub fn new(secret: impl Into<String>) -> Self {
        Self { secret: secret.into() }
    }
}

#[async_trait]
impl MiddleWareHandler for AuthMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        // 从请求头获取 token
        let token = req.headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "));

        match token {
            Some(token) => {
                // 验证 token（示例）
                if verify_token(token, &self.secret) {
                    next.call(req).await
                } else {
                    Err(SilentError::business_error(
                        StatusCode::UNAUTHORIZED,
                        "invalid token".to_string(),
                    ))
                }
            }
            None => Err(SilentError::business_error(
                StatusCode::UNAUTHORIZED,
                "missing authorization header".to_string(),
            )),
        }
    }
}
```

### 3. 注入数据到请求

```rust
#[derive(Clone)]
pub struct InjectUser;

#[async_trait]
impl MiddleWareHandler for InjectUser {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        // 通过 extensions 注入数据，处理器通过 req.extensions().get::<User>() 获取
        let user = User { id: 1, name: "admin".to_string() };
        req.extensions_mut().insert(user);

        next.call(req).await
    }
}
```

### 4. 修改响应

```rust
#[derive(Clone)]
pub struct AddHeaders;

#[async_trait]
impl MiddleWareHandler for AddHeaders {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let mut res = next.call(req).await?;
        res.headers_mut().insert("X-Powered-By", "Silent".parse().unwrap());
        Ok(res)
    }
}
```

### 5. 异常处理（吞掉错误转为响应）

```rust
use silent::middlewares::ExceptionHandler;

let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async move {
    match result {
        Ok(res) => Ok(res),
        Err(e) => {
            let status = e.status();
            Ok(Response::json(&serde_json::json!({
                "error": e.to_string(),
                "code": status.as_u16(),
            })).with_status(status))
        }
    }
});
```

## 中间件挂载方式

```rust
// 路由级挂载（链式）
let route = Route::new("api")
    .hook(AuthMiddleware::new("secret"))
    .hook(RequestLogger)
    .get(handler);

// 全局挂载（在根路由上）
let route = Route::new_root()
    .hook(Logger::new())
    .hook(Cors::new().origin("*"))
    .append(api_routes);
```

## 内置中间件一览

| 中间件 | 用途 | 示例 |
|--------|------|------|
| `Logger` | 结构化请求日志 | `Logger::new()` |
| `Cors` | 跨域资源共享 | `Cors::new().origin("*")` |
| `Timeout` | 请求超时控制 | `Timeout::new(Duration::from_secs(30))` |
| `RateLimiter` | 令牌桶限流 | `RateLimiter::per_second(100.0)` |
| `RequestId` | 请求追踪 ID | `RequestId::new()` |
| `Compression` | gzip/brotli 压缩 | `Compression::new()` |
| `ExceptionHandler` | 自定义异常处理 | 见上方示例 |

## 中间件执行顺序

中间件按 `hook` 调用顺序依次执行，形成链式调用：

```
请求 → Middleware1(前置) → Middleware2(前置) → Handler
响应 ← Middleware1(后置) ← Middleware2(后置) ← Handler
```
