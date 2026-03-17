use bytes::Bytes;
use http::HeaderValue;
use http::header::HeaderName;
use serde::Serialize;

use crate::core::req_body::ReqBody;
use crate::{Handler, Method, Request};

use super::TestResponse;

/// 集成测试客户端
///
/// 提供便捷的请求构建方法，直接调用路由而无需启动 TCP 服务器。
///
/// # 示例
///
/// ```rust
/// use silent::prelude::*;
/// use silent::testing::TestClient;
///
/// # async fn example() -> Result<()> {
/// let app = Route::new_root()
///     .append(Route::new("ping").get(|_: Request| async { Ok("pong") }));
///
/// let resp = TestClient::get("/ping").send(&app).await;
/// assert_eq!(resp.status(), StatusCode::OK);
/// # Ok(())
/// # }
/// ```
pub struct TestClient;

impl TestClient {
    /// 创建 GET 请求
    pub fn get(path: &str) -> TestRequest {
        TestRequest::new(Method::GET, path)
    }

    /// 创建 POST 请求
    pub fn post(path: &str) -> TestRequest {
        TestRequest::new(Method::POST, path)
    }

    /// 创建 PUT 请求
    pub fn put(path: &str) -> TestRequest {
        TestRequest::new(Method::PUT, path)
    }

    /// 创建 DELETE 请求
    pub fn delete(path: &str) -> TestRequest {
        TestRequest::new(Method::DELETE, path)
    }

    /// 创建 PATCH 请求
    pub fn patch(path: &str) -> TestRequest {
        TestRequest::new(Method::PATCH, path)
    }

    /// 创建自定义方法请求
    pub fn request(method: Method, path: &str) -> TestRequest {
        TestRequest::new(method, path)
    }
}

/// 测试请求构建器
///
/// 通过链式调用构建请求，最终调用 `send()` 发送到路由。
pub struct TestRequest {
    method: Method,
    uri: String,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Option<Bytes>,
}

impl TestRequest {
    pub(crate) fn new(method: Method, path: &str) -> Self {
        Self {
            method,
            uri: path.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// 添加请求头
    pub fn header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        V: TryInto<HeaderValue>,
    {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.headers.push((name, value));
        }
        self
    }

    /// 设置 JSON 请求体
    ///
    /// 自动设置 `Content-Type: application/json`。
    pub fn json<T: Serialize>(mut self, data: &T) -> Self {
        self.headers.push((
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        ));
        self.body = Some(Bytes::from(serde_json::to_vec(data).unwrap()));
        self
    }

    /// 设置表单请求体
    ///
    /// 自动设置 `Content-Type: application/x-www-form-urlencoded`。
    pub fn form(mut self, data: &str) -> Self {
        self.headers.push((
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        ));
        self.body = Some(Bytes::from(data.to_string()));
        self
    }

    /// 设置原始字节请求体
    pub fn body(mut self, data: impl Into<Bytes>) -> Self {
        self.body = Some(data.into());
        self
    }

    /// 设置文本请求体
    ///
    /// 自动设置 `Content-Type: text/plain`。
    pub fn text(mut self, data: &str) -> Self {
        self.headers.push((
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        ));
        self.body = Some(Bytes::from(data.to_string()));
        self
    }

    /// 发送请求到路由并返回测试响应
    ///
    /// 直接调用路由的 `Handler::call`，不经过网络层。
    pub async fn send<H: Handler>(self, handler: &H) -> TestResponse {
        let mut req = Request::empty();
        *req.method_mut() = self.method;
        *req.uri_mut() = self.uri.parse().expect("invalid URI");

        for (name, value) in self.headers {
            req.headers_mut().insert(name, value);
        }

        if let Some(body_bytes) = self.body {
            req.replace_body(ReqBody::Once(body_bytes));
        }

        // 设置默认 remote addr（部分中间件需要）
        req.set_remote("127.0.0.1:0".parse().unwrap());

        match handler.call(req).await {
            Ok(response) => TestResponse::from_response(response).await,
            Err(err) => TestResponse::from_error(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[tokio::test]
    async fn test_get_request() {
        let app =
            Route::new_root().append(Route::new("hello").get(|_: Request| async { Ok("Hello!") }));

        let resp = TestClient::get("/hello").send(&app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.text().await, "Hello!");
    }

    #[tokio::test]
    async fn test_post_request() {
        let app =
            Route::new_root().append(Route::new("echo").post(|_: Request| async { Ok("posted") }));

        let resp = TestClient::post("/echo").send(&app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.text().await, "posted");
    }

    #[tokio::test]
    async fn test_custom_header() {
        let app = Route::new_root().append(Route::new("h").get(|req: Request| async move {
            let val = req
                .headers()
                .get("x-custom")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("none")
                .to_string();
            Ok(val)
        }));

        let resp = TestClient::get("/h")
            .header("x-custom", "test-value")
            .send(&app)
            .await;
        assert_eq!(resp.text().await, "test-value");
    }

    #[tokio::test]
    async fn test_json_body() {
        use serde::Deserialize;

        #[derive(Serialize, Deserialize)]
        struct Input {
            name: String,
        }

        let app = Route::new_root().append(Route::new("json").post(|req: Request| async move {
            let body = hyper::body::Bytes::from(
                req.headers()
                    .get("content-type")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
            Ok(Response::text(std::str::from_utf8(&body).unwrap()))
        }));

        let resp = TestClient::post("/json")
            .json(&Input {
                name: "Alice".to_string(),
            })
            .send(&app)
            .await;
        assert!(resp.text().await.contains("application/json"));
    }

    #[tokio::test]
    async fn test_not_found() {
        let app =
            Route::new_root().append(Route::new("exists").get(|_: Request| async { Ok("ok") }));

        let resp = TestClient::get("/not-exists").send(&app).await;
        assert_ne!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_all_methods() {
        let app = Route::new_root().append(
            Route::new("m")
                .get(|_: Request| async { Ok("get") })
                .post(|_: Request| async { Ok("post") })
                .put(|_: Request| async { Ok("put") })
                .delete(|_: Request| async { Ok("delete") })
                .patch(|_: Request| async { Ok("patch") }),
        );

        assert_eq!(TestClient::get("/m").send(&app).await.text().await, "get");
        assert_eq!(TestClient::post("/m").send(&app).await.text().await, "post");
        assert_eq!(TestClient::put("/m").send(&app).await.text().await, "put");
        assert_eq!(
            TestClient::delete("/m").send(&app).await.text().await,
            "delete"
        );
        assert_eq!(
            TestClient::patch("/m").send(&app).await.text().await,
            "patch"
        );
    }

    #[tokio::test]
    async fn test_with_middleware() {
        use crate::middlewares::RequestId;

        let app = Route::new_root().append(
            Route::new("mid")
                .hook(RequestId::new())
                .get(|_: Request| async { Ok("ok") }),
        );

        let resp = TestClient::get("/mid").send(&app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn test_form_body() {
        let app = Route::new_root().append(Route::new("form").post(|req: Request| async move {
            let ct = req
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            Ok(ct)
        }));

        let resp = TestClient::post("/form").form("key=value").send(&app).await;
        assert!(resp.text().await.contains("x-www-form-urlencoded"));
    }

    #[tokio::test]
    async fn test_text_body() {
        let app = Route::new_root().append(Route::new("txt").post(|req: Request| async move {
            let ct = req
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            Ok(ct)
        }));

        let resp = TestClient::post("/txt").text("hello").send(&app).await;
        assert!(resp.text().await.contains("text/plain"));
    }
}
