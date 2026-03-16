use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError, StatusCode};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tera::{Context, Tera};

#[derive(Debug, Clone)]
pub struct TemplateResponse {
    template: String,
    data: Value,
}

impl<T: Serialize, S: Into<String>> From<(S, T)> for TemplateResponse {
    fn from((template, data): (S, T)) -> Self {
        let template = template.into();
        let data = serde_json::to_value(data).unwrap_or(Value::Null);
        TemplateResponse { template, data }
    }
}

impl From<TemplateResponse> for Response {
    fn from(value: TemplateResponse) -> Self {
        let mut res = Response::empty();
        res.extensions.insert(value);
        res
    }
}

pub struct TemplateMiddleware {
    pub template: Arc<Tera>,
}

impl TemplateMiddleware {
    pub fn try_new(template_path: &str) -> Result<Self> {
        let template = Arc::new(Tera::new(template_path).map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load templates: {e}"),
            )
        })?);
        Ok(TemplateMiddleware { template })
    }

    pub fn new(template_path: &str) -> Self {
        Self::try_new(template_path).expect("Failed to load templates")
    }
}

#[async_trait]
impl MiddleWareHandler for TemplateMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let mut res = next.call(req).await?;
        let template = res.extensions.get::<TemplateResponse>().ok_or_else(|| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "template response missing",
            )
        })?;
        res.set_body(
            self.template
                .render(
                    &template.template,
                    &Context::from_serialize(&template.data).map_err(|e| {
                        SilentError::business_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to render template: {e}"),
                        )
                    })?,
                )
                .map_err(|e| {
                    SilentError::business_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to render template: {e}"),
                    )
                })?
                .into(),
        );
        res.set_typed_header(headers::ContentType::html());
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Route;
    use crate::{Handler, Request};
    use bytes::Bytes;
    use http_body_util::BodyExt;

    // ==================== TemplateResponse 测试 ====================

    #[test]
    fn test_template_response_from_tuple() {
        let resp = TemplateResponse::from(("index.html", serde_json::json!({"key": "val"})));
        assert_eq!(resp.template, "index.html");
        assert_eq!(resp.data["key"], "val");
    }

    #[test]
    fn test_template_response_from_string_template_name() {
        let resp = TemplateResponse::from(("page.html".to_string(), vec![1, 2, 3]));
        assert_eq!(resp.template, "page.html");
        assert!(resp.data.is_array());
    }

    #[test]
    fn test_template_response_clone() {
        let resp = TemplateResponse::from(("t.html", "data"));
        let cloned = resp.clone();
        assert_eq!(resp.template, cloned.template);
        assert_eq!(resp.data, cloned.data);
    }

    #[test]
    fn test_template_response_into_response() {
        let template_resp = TemplateResponse::from(("t.html", "hello"));
        let response: Response = template_resp.into();
        // Response 的 extensions 应包含 TemplateResponse
        assert!(response.extensions.get::<TemplateResponse>().is_some());
    }

    // ==================== TemplateMiddleware 构造测试 ====================

    #[test]
    fn test_try_new_invalid_glob_pattern() {
        // Tera 对无效 glob 模式报错（如缺少 * 的模式）
        let result = TemplateMiddleware::try_new("[invalid glob");
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "Failed to load templates")]
    fn test_new_invalid_glob_panics() {
        let _ = TemplateMiddleware::new("[invalid glob");
    }

    // ==================== MiddleWareHandler 错误路径测试 ====================

    #[tokio::test]
    async fn test_handle_missing_template_response() {
        let mut tera = Tera::default();
        tera.add_raw_template("t.html", "hi").unwrap();
        let mid = TemplateMiddleware {
            template: Arc::new(tera),
        };
        // handler 返回普通 Response（没有 TemplateResponse extension）
        let route = Route::default()
            .get(|_req: Request| async { Ok(Response::text("no template")) })
            .hook(mid);
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
        let res = route.call(req).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_handle_unknown_template_name() {
        let mut tera = Tera::default();
        tera.add_raw_template("known.html", "ok").unwrap();
        let mid = TemplateMiddleware {
            template: Arc::new(tera),
        };
        // handler 返回引用不存在模板名的 TemplateResponse
        let route = Route::default()
            .get(|_req: Request| async { Ok(TemplateResponse::from(("unknown.html", "data"))) })
            .hook(mid);
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
        let res = route.call(req).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_handle_sets_content_type_html() {
        let mut tera = Tera::default();
        tera.add_raw_template("t.html", "<p>{{ v }}</p>").unwrap();
        let mid = TemplateMiddleware {
            template: Arc::new(tera),
        };
        let route = Route::default()
            .get(|_req: Request| async {
                Ok(TemplateResponse::from((
                    "t.html",
                    serde_json::json!({"v": "x"}),
                )))
            })
            .hook(mid);
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
        let res = route.call(req).await.unwrap();
        let ct = res
            .headers()
            .get(http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.contains("text/html"));
    }

    // ==================== 成功路径测试（原有） ====================

    #[derive(Serialize)]
    struct Temp {
        name: String,
    }

    #[tokio::test]
    async fn templates_test() {
        let mut tera = Tera::default();
        tera.add_raw_template("index.html", "<h1>{{ name }}</h1>")
            .unwrap();
        let temp_middleware = TemplateMiddleware {
            template: Arc::new(tera),
        };
        let route = Route::default()
            .get(|_req: Request| async {
                let temp = Temp {
                    name: "templates".to_string(),
                };
                Ok(TemplateResponse::from(("index.html".to_string(), temp)))
            })
            .hook(temp_middleware);
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
        assert_eq!(
            route
                .call(req)
                .await
                .unwrap()
                .body
                .frame()
                .await
                .unwrap()
                .unwrap()
                .data_ref()
                .unwrap(),
            &Bytes::from("<h1>templates</h1>")
        );
    }
}
