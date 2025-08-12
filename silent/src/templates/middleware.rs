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
        serde_json::to_value(data)
            .map(|data| TemplateResponse { template, data })
            .unwrap()
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
    pub fn new(template_path: &str) -> Self {
        let template = Arc::new(Tera::new(template_path).expect("Failed to load templates"));
        TemplateMiddleware { template }
    }
}

#[async_trait]
impl MiddleWareHandler for TemplateMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let mut res = next.call(req).await?;
        let template = res.extensions.get::<TemplateResponse>().unwrap();
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
