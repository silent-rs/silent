# Silent 萃取器指南

## 什么是萃取器？

萃取器（Extractors）是 Silent 框架中用于从 HTTP 请求中提取数据的核心机制。它允许您以类型安全的方式获取路径参数、查询参数、请求头、请求体等各种数据。

萃取器的设计灵感来自 Axum，但提供了更灵活的类型系统和更强大的功能。

## 萃取器的工作原理

萃取器实现了 `FromRequest` trait，该 trait 定义了如何从请求中提取特定类型的数据：

```rust
#[async_trait]
pub trait FromRequest: Sized {
    type Rejection: Into<Response> + Send + 'static;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection>;
}
```

当处理函数使用萃取器参数时，框架会自动调用 `from_request` 方法提取数据。如果提取失败，会返回相应的错误。

## 内置萃取器

Silent 提供了丰富的内置萃取器，满足各种数据提取需求：

### 1. Path - 路径参数萃取器

用于从 URL 路径中提取参数，支持单值和结构体两种方式。

#### 单值提取

```rust
// 提取单个路径参数
async fn handler(Path(id): Path<i64>) -> Result<String> {
    Ok(format!("用户ID: {}", id))
}
```

路由定义：
```rust
Route::new("users/<id>").get(handler)
```

#### 结构体提取

```rust
#[derive(Deserialize)]
struct UserPath {
    id: i64,
    name: String,
}

async fn handler(Path(user): Path<UserPath>) -> Result<String> {
    Ok(format!("用户: {} (ID: {})", user.name, user.id))
}
```

路由定义：
```rust
Route::new("users/<id>/<name>").get(handler)
```

### 2. Query - 查询参数萃取器

从 URL 查询字符串中提取参数。

#### 单个查询参数

```rust
#[derive(Deserialize)]
struct Page {
    page: u32,
    size: u32,
}

async fn handler(Query(p): Query<Page>) -> Result<String> {
    Ok(format!("第 {} 页，每页 {} 条", p.page, p.size))
}
```

路由定义：
```rust
Route::new("items").get(handler)
```

访问：`/items?page=1&size=20`

#### 多个查询参数

```rust
#[derive(Deserialize)]
struct Search {
    keyword: Option<String>,
}

async fn handler((Query(s), Query(p)): (Query<Search>, Query<Page>)) -> Result<String> {
    let keyword = s.keyword.as_deref().unwrap_or("无");
    Ok(format!("搜索: {}，第 {} 页", keyword, p.page))
}
```

### 3. Json - JSON 请求体萃取器

从 `application/json` 请求体中提取数据。

```rust
#[derive(Deserialize, Serialize)]
struct CreateUser {
    name: String,
    age: u32,
}

async fn handler(Json(user): Json<CreateUser>) -> Result<String> {
    Ok(format!("创建用户: {} ({})", user.name, user.age))
}
```

路由定义：
```rust
Route::new("users").post(handler)
```

请求示例：
```bash
curl -X POST http://localhost/users \
  -H "Content-Type: application/json" \
  -d '{"name": "张三", "age": 25}'
```

### 4. Form - 表单数据萃取器

从 `application/x-www-form-urlencoded` 请求体中提取数据。

```rust
#[derive(Deserialize, Serialize)]
struct LoginForm {
    username: String,
    password: String,
}

async fn handler(Json(form): Json<LoginForm>) -> Result<String> {
    Ok(format!("用户登录: {}", form.username))
}
```

### 5. TypedHeader - 类型化请求头萃取器

提取并解析特定类型的请求头。

```rust
use silent::headers::UserAgent;

async fn handler(TypedHeader(ua): TypedHeader<UserAgent>) -> Result<String> {
    Ok(format!("用户代理: {}", ua.as_str()))
}
```

### 6. Method、Uri、Version - 基础信息萃取器

提取请求方法、URI 和 HTTP 版本。

```rust
async fn handler(
    Method(m): Method,
    Uri(u): Uri,
    Version(v): Version,
) -> Result<String> {
    Ok(format!("{} {} {:?}", m, u.path(), v))
}
```

### 7. Extension - 扩展数据萃取器

从请求扩展中提取数据，通常用于中间件注入的数据。

```rust
#[derive(Clone)]
struct UserId(u64);

async fn handler(Extension(user_id): Extension<UserId>) -> Result<String> {
    Ok(format!("用户ID: {}", user_id.0))
}
```

在中间件中注入数据：
```rust
impl MiddleWareHandler for InjectUserId {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        req.extensions_mut().insert(UserId(123));
        next.call(req).await
    }
}
```

### 8. Configs - 配置萃取器

从请求配置中提取数据，通常用于全局配置。

```rust
#[derive(Clone)]
struct AppConfig {
    name: String,
    version: String,
}

async fn handler(Configs(config): Configs<AppConfig>) -> Result<String> {
    Ok(format!("应用: {} v{}", config.name, config.version))
}
```

在路由中注入配置：
```rust
let route = Route::new("api")
    .with_config(AppConfig {
        name: "MyApp".to_string(),
        version: "1.0.0".to_string(),
    })
    .append(Route::new("info").get(handler));
```

## 单个字段萃取器

单个字段萃取器允许您直接提取单个字段，而无需创建结构体。这对于只需要一两个参数的情况非常有用。

### QueryParam - 按名称提取查询参数

```rust
async fn handler(mut req: Request) -> Result<String> {
    let name = query_param::<String>(&mut req, "name").await.unwrap_or_default();
    let age = query_param::<u32>(&mut req, "age").await.unwrap_or(0);

    Ok(format!("姓名: {}, 年龄: {}", name, age))
}
```

### PathParam - 按名称提取路径参数

```rust
async fn handler(mut req: Request) -> Result<String> {
    let id = path_param::<i64>(&mut req, "id").await.unwrap_or_default();
    Ok(format!("ID: {}", id))
}
```

### HeaderParam - 按名称提取请求头

```rust
async fn handler(mut req: Request) -> Result<String> {
    let auth = header_param::<String>(&mut req, "authorization")
        .await
        .unwrap_or_default();

    Ok(format!("认证: {}", auth))
}
```

### CookieParam - 按名称提取 Cookie

```rust
async fn handler(mut req: Request) -> Result<String> {
    let session = cookie_param::<String>(&mut req, "session")
        .await
        .unwrap_or_default();

    Ok(format!("会话: {}", session))
}
```

### ConfigParam - 按类型提取配置

```rust
#[derive(Clone)]
struct DatabaseConfig {
    url: String,
}

async fn handler(mut req: Request) -> Result<String> {
    let config = config_param::<DatabaseConfig>(&mut req).await.unwrap();
    Ok(format!("数据库: {}", config.url))
}
```

## 类型转换

所有萃取器都支持丰富的类型转换：

### 基本类型

```rust
// 整数类型
let id = query_param::<i32>(&mut req, "id").await.unwrap();
let count = query_param::<u64>(&mut req, "count").await.unwrap();

// 浮点类型
let price = query_param::<f64>(&mut req, "price").await.unwrap();

// 布尔类型
let active = query_param::<bool>(&mut req, "active").await.unwrap();

// 字符串
let name = query_param::<String>(&mut req, "name").await.unwrap();
```

### 枚举类型

```rust
#[derive(Deserialize)]
enum Role {
    Admin,
    User,
    Guest,
}

let role = query_param::<Role>(&mut req, "role").await.unwrap();
```

### DateTime 类型

```rust
use chrono::{DateTime, Utc};

let created_at = query_param::<DateTime<Utc>>(&mut req, "created_at")
    .await
    .unwrap();
```

### 自定义类型

只要实现了 `serde::Deserialize`，就可以用于萃取器：

```rust
#[derive(Deserialize)]
struct Address {
    street: String,
    city: String,
    zip_code: String,
}

let address = query_param::<Address>(&mut req, "address").await.unwrap();
```

## 多萃取器组合

萃取器可以组合使用，同时提取多种数据：

### 元组组合

```rust
async fn handler(
    (Path(id), Query(p), Json(data)): (Path<i64>, Query<Page>, Json<Data>),
) -> Result<String> {
    // 处理提取的数据
    Ok("成功")
}
```

### 与 Request 组合

```rust
async fn handler(
    req: Request,
    (Path(id), Query(p)): (Path<i64>, Query<Page>),
) -> Result<String> {
    // 可以访问完整的 Request 对象
    let ip = req.remote();
    Ok(format!("IP: {}, ID: {}", ip, id))
}
```

## Option 和 Result 包装器

### Option<T> - 可选参数

当参数可能不存在时，使用 `Option<T>`：

```rust
async fn handler(opt_id: Option<Path<i64>>) -> Result<String> {
    match opt_id {
        Some(Path(id)) => Ok(format!("有ID: {}", id)),
        None => Ok("无ID".to_string()),
    }
}
```

### Result<T, E> - 错误处理

当需要自定义错误处理时，使用 `Result<T, Response>`：

```rust
async fn handler(
    result: Result<Json<Data>, Response>,
) -> Result<String> {
    match result {
        Ok(Json(data)) => Ok(format!("数据: {:?}", data)),
        Err(response) => Ok("请求无效".to_string()),
    }
}
```

## 自定义萃取器

您也可以创建自己的萃取器：

```rust
use async_trait::async_trait;

struct AuthToken(String);

#[async_trait]
impl FromRequest for AuthToken {
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let token = req.headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body("Missing auth token".into())
                    .unwrap()
            })?;

        Ok(AuthToken(token))
    }
}

// 使用自定义萃取器
async fn handler(AuthToken(token): AuthToken) -> Result<String> {
    Ok(format!("Token: {}", token))
}
```

## 错误处理

萃取器在提取失败时会返回错误：

### SilentError 常见错误

- `ParamsEmpty` - 参数为空
- `ParamsNotFound` - 参数未找到
- `ParseError` - 解析错误

### 自定义错误处理

```rust
async fn handler(mut req: Request) -> Result<String> {
    match query_param::<String>(&mut req, "required").await {
        Ok(value) => Ok(format!("获取成功: {}", value)),
        Err(_) => Ok("缺少必需参数".to_string()),
    }
}
```

## 最佳实践

### 1. 选择合适的萃取器类型

- **单个简单参数**：使用单个字段萃取器（QueryParam、PathParam 等）
- **相关参数组合**：使用结构体萃取器（Query<T>、Path<T> 等）
- **复杂请求体**：使用 Json<T> 或 Form<T>
- **可选参数**：使用 `Option<T>`

### 2. 验证输入数据

```rust
#[derive(Deserialize)]
struct CreateUser {
    name: String,
    age: u32,
}

async fn handler(Json(user): Json<CreateUser>) -> Result<String> {
    if user.age < 18 {
        return Err(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("年龄必须大于等于18岁".into())
            .unwrap());
    }

    Ok(format!("创建用户成功"))
}
```

### 3. 使用中间件共享数据

```rust
impl MiddleWareHandler for AuthMiddleware {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        // 验证用户身份
        if let Some(user_id) = validate_user(&req) {
            req.extensions_mut().insert(UserId(user_id));
        }

        next.call(req).await
    }
}
```

### 4. 错误信息本地化

```rust
async fn handler(Path(id): Path<i64>) -> Result<String> {
    if id <= 0 {
        return Err(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("ID必须是正整数".into())
            .unwrap());
    }

    Ok(format!("用户ID: {}", id))
}
```

## 与 Axum 对比

| 特性 | Silent | Axum |
|------|--------|------|
| 类型安全 | ✅ 完整支持 | ✅ 完整支持 |
| 零成本抽象 | ✅ | ✅ |
| 单个字段萃取 | ✅ | ✅ |
| 元组组合 | ✅ 支持最多4个 | ✅ 无限制 |
| Option/Result 支持 | ✅ | ✅ |
| 自定义萃取器 | ✅ | ✅ |
| 扩展数据 | ✅ Extension | ✅ Extension |

## 总结

Silent 的萃取器系统提供了类型安全、灵活且高性能的数据提取方案。通过合理使用内置萃取器和自定义萃取器，您可以构建清晰、可维护的请求处理逻辑。

建议在开发过程中：
1. 根据数据特点选择合适的萃取器
2. 利用类型系统保证数据安全
3. 适当使用中间件共享数据
4. 提供友好的错误处理和验证

更多示例请参考 `examples/extractors/` 目录。
