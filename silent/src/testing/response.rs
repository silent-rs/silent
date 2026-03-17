use bytes::Bytes;
use http::HeaderMap;
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;

use crate::{Response, SilentError, StatusCode};

/// 测试响应包装器
///
/// 封装 HTTP 响应，提供便捷的读取和断言方法。
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl TestResponse {
    /// 从成功的 Response 构建（异步收集 body）
    pub(crate) async fn from_response(mut response: Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.take_body();

        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => Bytes::new(),
        };

        Self {
            status,
            headers,
            body: body_bytes,
        }
    }

    /// 从错误构建
    pub(crate) fn from_error(err: SilentError) -> Self {
        Self {
            status: err.status(),
            headers: HeaderMap::new(),
            body: Bytes::from(err.to_string()),
        }
    }

    // ==================== 读取方法 ====================

    /// 获取 HTTP 状态码
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// 获取响应头
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// 获取响应体字节（消耗 self）
    pub async fn bytes(self) -> Bytes {
        self.body
    }

    /// 获取响应体字符串（消耗 self）
    pub async fn text(self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// 解析响应体为 JSON（消耗 self）
    pub async fn json<T: DeserializeOwned>(self) -> T {
        serde_json::from_slice(&self.body).expect("failed to parse response body as JSON")
    }

    // ==================== 断言方法 ====================

    /// 断言状态码
    pub fn assert_status(self, expected: StatusCode) -> Self {
        assert_eq!(
            self.status, expected,
            "expected status {}, got {}",
            expected, self.status
        );
        self
    }

    /// 断言响应头存在且值匹配
    pub fn assert_header(self, name: &str, expected: &str) -> Self {
        let value = self
            .headers
            .get(name)
            .unwrap_or_else(|| panic!("header '{}' not found", name))
            .to_str()
            .unwrap_or_else(|_| panic!("header '{}' is not valid UTF-8", name));
        assert_eq!(
            value, expected,
            "header '{}': expected '{}', got '{}'",
            name, expected, value
        );
        self
    }

    /// 断言响应头存在
    pub fn assert_header_exists(self, name: &str) -> Self {
        assert!(
            self.headers.get(name).is_some(),
            "expected header '{}' to exist",
            name
        );
        self
    }

    /// 断言响应体包含指定子串
    pub fn assert_body_contains(self, substring: &str) -> Self {
        let body_str = String::from_utf8_lossy(&self.body);
        assert!(
            body_str.contains(substring),
            "expected body to contain '{}', body was: '{}'",
            substring,
            body_str
        );
        self
    }

    /// 断言响应体等于指定字符串
    pub fn assert_body_eq(self, expected: &str) -> Self {
        let body_str = String::from_utf8_lossy(&self.body);
        assert_eq!(
            body_str, expected,
            "expected body '{}', got '{}'",
            expected, body_str
        );
        self
    }

    /// 断言 JSON 响应体与期望值相等
    pub fn assert_json<T: DeserializeOwned + PartialEq + std::fmt::Debug>(
        self,
        expected: &T,
    ) -> Self {
        let actual: T =
            serde_json::from_slice(&self.body).expect("failed to parse response body as JSON");
        assert_eq!(actual, *expected);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::testing::TestClient;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct User {
        id: u64,
        name: String,
    }

    #[tokio::test]
    async fn test_assert_status() {
        let app = Route::new_root().append(Route::new("ok").get(|_: Request| async { Ok("ok") }));

        TestClient::get("/ok")
            .send(&app)
            .await
            .assert_status(StatusCode::OK);
    }

    #[tokio::test]
    async fn test_assert_body_eq() {
        let app = Route::new_root()
            .append(Route::new("msg").get(|_: Request| async { Ok("hello world") }));

        TestClient::get("/msg")
            .send(&app)
            .await
            .assert_body_eq("hello world");
    }

    #[tokio::test]
    async fn test_assert_body_contains() {
        let app = Route::new_root()
            .append(Route::new("msg").get(|_: Request| async { Ok("hello world") }));

        TestClient::get("/msg")
            .send(&app)
            .await
            .assert_body_contains("world");
    }

    #[tokio::test]
    async fn test_assert_header_exists() {
        let app = Route::new_root().append(Route::new("h").get(|_: Request| async {
            Ok(Response::text("ok").with_header(
                http::header::HeaderName::from_static("x-test"),
                http::header::HeaderValue::from_static("yes"),
            ))
        }));

        TestClient::get("/h")
            .send(&app)
            .await
            .assert_header_exists("x-test")
            .assert_header("x-test", "yes");
    }

    #[tokio::test]
    async fn test_json_response() {
        let user = User {
            id: 1,
            name: "Alice".to_string(),
        };

        let app = Route::new_root().append(Route::new("user").get(|_: Request| async {
            Ok(Response::json(&User {
                id: 1,
                name: "Alice".to_string(),
            }))
        }));

        let resp = TestClient::get("/user").send(&app).await;
        let got: User = resp.json().await;
        assert_eq!(got, user);
    }

    #[tokio::test]
    async fn test_assert_json() {
        let expected = User {
            id: 42,
            name: "Bob".to_string(),
        };

        let app = Route::new_root().append(Route::new("u").get(|_: Request| async {
            Ok(Response::json(&User {
                id: 42,
                name: "Bob".to_string(),
            }))
        }));

        TestClient::get("/u")
            .send(&app)
            .await
            .assert_status(StatusCode::OK)
            .assert_json(&expected);
    }

    #[tokio::test]
    async fn test_chained_assertions() {
        let app = Route::new_root().append(Route::new("chain").get(|_: Request| async {
            Ok(Response::text("chained").with_header(
                http::header::HeaderName::from_static("x-chain"),
                http::header::HeaderValue::from_static("val"),
            ))
        }));

        TestClient::get("/chain")
            .send(&app)
            .await
            .assert_status(StatusCode::OK)
            .assert_header("x-chain", "val")
            .assert_body_contains("chain");
    }

    #[tokio::test]
    async fn test_error_response() {
        let app = Route::new_root().append(Route::new("err").get(|_: Request| async {
            Err::<Response, _>(SilentError::business_error(
                StatusCode::BAD_REQUEST,
                "bad request".to_string(),
            ))
        }));

        let resp = TestClient::get("/err").send(&app).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_bytes_response() {
        let app =
            Route::new_root().append(Route::new("b").get(|_: Request| async { Ok("raw bytes") }));

        let resp = TestClient::get("/b").send(&app).await;
        let data = resp.bytes().await;
        assert_eq!(data, Bytes::from("raw bytes"));
    }
}
