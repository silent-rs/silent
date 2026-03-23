//! Tower Layer → Silent MiddleWareHandler 适配器
//!
//! 通过 `Route::hook_layer()` 方法，将任意 `tower::Layer` 隐式适配为 Silent 中间件。
//!
//! # 示例
//!
//! ```rust,ignore
//! use tower::layer::layer_fn;
//!
//! let route = Route::new("api")
//!     .hook_layer(some_tower_layer)
//!     .get(handler);
//! ```

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use http::Response as HttpResponse;
use http_body::Body;
use tower::{Service, ServiceExt};

use crate::core::next::Next;
use crate::core::req_body::ReqBody;
use crate::core::res_body::ResBody;
use crate::error::BoxedError;
use crate::{Handler, MiddleWareHandler, Request, Response, SilentError, StatusCode};

/// Silent 特有数据，在转换为 http::Request 时存入 Extensions，
/// 在 NextService 中恢复。
#[derive(Clone)]
struct SilentExtras {
    state: crate::State,
    path_params: std::collections::HashMap<String, crate::core::path_param::PathParam>,
}

/// 将 Tower Layer 适配为 Silent MiddleWareHandler。
///
/// 用户不需要直接使用此类型，通过 `Route::hook_layer()` 自动创建。
#[doc(hidden)]
pub struct TowerLayerAdapter<L> {
    layer: L,
}

impl<L> TowerLayerAdapter<L> {
    #[doc(hidden)]
    pub fn new(layer: L) -> Self {
        Self { layer }
    }
}

/// 将 Silent 的 Next 包装为 tower::Service，
/// 供 Tower 中间件作为内层 Service 调用。
///
/// 用户不需要直接使用此类型。
#[derive(Clone)]
#[doc(hidden)]
pub struct NextServicePublic {
    pub(crate) next: Next,
}

impl Service<http::Request<ReqBody>> for NextServicePublic {
    type Response = HttpResponse<ResBody>;
    type Error = BoxedError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let next = self.next.clone();
        Box::pin(async move {
            // 从 http::Request 恢复 Silent Request
            let silent_req = from_http_request(req);

            // 调用 Silent 中间件链的下一层
            let silent_res = next
                .call(silent_req)
                .await
                .map_err(|e| -> BoxedError { Box::new(e) })?;

            // 将 Silent Response 转为 http::Response
            Ok(into_http_response(silent_res))
        })
    }
}

#[async_trait]
impl<L> MiddleWareHandler for TowerLayerAdapter<L>
where
    L: tower::Layer<NextServicePublic> + Clone + Send + Sync + 'static,
    L::Service: Service<http::Request<ReqBody>> + Clone + Send + 'static,
    <L::Service as Service<http::Request<ReqBody>>>::Response: IntoSilentResponse + Send,
    <L::Service as Service<http::Request<ReqBody>>>::Error: Into<BoxedError> + Send,
    <L::Service as Service<http::Request<ReqBody>>>::Future: Send,
{
    async fn handle(&self, req: Request, next: &Next) -> crate::Result<Response> {
        let next_svc = NextServicePublic { next: next.clone() };
        let svc = self.layer.clone().layer(next_svc);

        // 将 Silent Request 转为 http::Request，保存特有数据
        let http_req = into_http_request(req);

        // 通过 oneshot 调用 Tower Service（自动处理 poll_ready）
        let tower_res = svc.oneshot(http_req).await.map_err(|e| {
            let err: BoxedError = e.into();
            SilentError::business_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })?;

        // 将结果转回 Silent Response
        Ok(tower_res.into_silent_response())
    }
}

// ==================== 类型转换辅助 ====================

/// 将 Silent Request 转为 http::Request，将 Silent 特有数据存入 Extensions
fn into_http_request(req: Request) -> http::Request<ReqBody> {
    let state = req.state();
    let path_params = req.path_params().clone();

    let mut http_req = req.into_http();

    // 将 Silent 特有数据存入 Extensions
    http_req
        .extensions_mut()
        .insert(SilentExtras { state, path_params });

    http_req
}

/// 从 http::Request 恢复 Silent Request，从 Extensions 中提取特有数据
fn from_http_request(mut req: http::Request<ReqBody>) -> Request {
    let extras = req.extensions_mut().remove::<SilentExtras>();

    let (parts, body) = req.into_parts();
    let mut silent_req = Request::from_parts(parts, body);

    if let Some(extras) = extras {
        *silent_req.state_mut() = extras.state;
        for (key, value) in extras.path_params {
            silent_req.set_path_params(key, value);
        }
    }

    silent_req
}

/// 将 Silent Response 转为 http::Response<ResBody>
fn into_http_response(mut res: Response) -> HttpResponse<ResBody> {
    let body = res.take_body();
    let mut builder = HttpResponse::builder()
        .status(res.status)
        .version(res.version);

    if let Some(headers) = builder.headers_mut() {
        *headers = std::mem::take(&mut res.headers);
    }

    builder.body(body).unwrap()
}

/// 将任意 Tower 中间件的响应转回 Silent Response 的 trait
#[doc(hidden)]
pub trait IntoSilentResponse {
    fn into_silent_response(self) -> Response;
}

/// 当 Tower 中间件返回任意 Body 类型时，通过 ResBody::Boxed 包装
impl<B> IntoSilentResponse for HttpResponse<B>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxedError> + 'static,
{
    fn into_silent_response(self) -> Response {
        use http_body_util::BodyExt;
        let (parts, body) = self.into_parts();
        let mapped = body.map_err(|e| -> BoxedError { e.into() });
        let res_body = ResBody::Boxed(Box::pin(mapped));
        let mut res = Response::empty();
        res.set_status(parts.status);
        res.version = parts.version;
        res.headers = parts.headers;
        res.set_body(res_body);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Handler;
    use crate::route::Route;

    /// 一个简单的 Tower Layer，为响应添加自定义 header
    #[derive(Clone)]
    struct AddHeaderLayer {
        name: &'static str,
        value: &'static str,
    }

    impl<S: Clone> tower::Layer<S> for AddHeaderLayer {
        type Service = AddHeaderService<S>;
        fn layer(&self, inner: S) -> Self::Service {
            AddHeaderService {
                inner,
                name: self.name,
                value: self.value,
            }
        }
    }

    #[derive(Clone)]
    struct AddHeaderService<S> {
        inner: S,
        name: &'static str,
        value: &'static str,
    }

    impl<S> Service<http::Request<ReqBody>> for AddHeaderService<S>
    where
        S: Service<http::Request<ReqBody>, Response = HttpResponse<ResBody>, Error = BoxedError>
            + Clone
            + Send
            + 'static,
        S::Future: Send,
    {
        type Response = HttpResponse<ResBody>;
        type Error = BoxedError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
            let mut inner = self.inner.clone();
            let name = self.name;
            let value = self.value;
            Box::pin(async move {
                let mut res = inner.call(req).await?;
                res.headers_mut()
                    .insert(name, http::HeaderValue::from_static(value));
                Ok(res)
            })
        }
    }

    #[tokio::test]
    async fn test_tower_layer_adds_header() {
        let layer = AddHeaderLayer {
            name: "x-tower-test",
            value: "hello",
        };

        let route = Route::new("")
            .hook_layer(layer)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res = route.call(req).await.unwrap();

        assert_eq!(
            res.headers().get("x-tower-test").unwrap().to_str().unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn test_tower_layer_preserves_state() {
        let layer = AddHeaderLayer {
            name: "x-check",
            value: "passed",
        };

        let route =
            Route::new("")
                .with_state(42i32)
                .hook_layer(layer)
                .get(|req: Request| async move {
                    let num = req.get_state::<i32>()?;
                    Ok(format!("state={}", num))
                });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res = route.call(req).await.unwrap();

        // Tower 中间件添加的 header 存在
        assert!(res.headers().get("x-check").is_some());
    }

    #[tokio::test]
    async fn test_tower_layer_chain() {
        let layer1 = AddHeaderLayer {
            name: "x-first",
            value: "1",
        };
        let layer2 = AddHeaderLayer {
            name: "x-second",
            value: "2",
        };

        let route = Route::new("")
            .hook_layer(layer1)
            .hook_layer(layer2)
            .get(|_req: Request| async { Ok("chained") });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res = route.call(req).await.unwrap();

        assert_eq!(res.headers().get("x-first").unwrap().to_str().unwrap(), "1");
        assert_eq!(
            res.headers().get("x-second").unwrap().to_str().unwrap(),
            "2"
        );
    }
}
