#![cfg(all(feature = "worker", target_arch = "wasm32"))]

use bytes::Bytes;
use http::Request as HttpRequest;
use http_body_util::BodyExt;
use worker::{Headers as WHeaders, Request as WRequest, Response as WResponse};

use crate::core::req_body::ReqBody;
use crate::core::request::Request as SRequest;
use crate::core::res_body::ResBody;
use crate::core::response::Response as SResponse;
use crate::handler::Handler;
use crate::route::Route;

/// Cloudflare Workers 适配路由
pub struct WorkRoute {
    pub route: Route,
}

impl WorkRoute {
    pub fn new(route: Route) -> Self {
        Self { route }
    }

    /// 处理 Cloudflare Worker 请求
    pub async fn call(&self, req: WRequest) -> WResponse {
        match self.handle(req).await {
            Ok(resp) => resp,
            Err(e) => WResponse::error(format!("internal error: {e}"), 500).unwrap(),
        }
    }

    async fn handle(&self, req: WRequest) -> worker::Result<WResponse> {
        let sreq = to_silent_request(req).await?;
        let mut sres = match self.route.call(sreq).await {
            Ok(r) => r,
            Err(e) => {
                let mut r = SResponse::empty();
                r.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
                r.set_body(ResBody::from(e.to_string()));
                r
            }
        };
        to_worker_response(&mut sres).await
    }
}

async fn to_silent_request(mut req: WRequest) -> worker::Result<SRequest> {
    // method
    let method: http::Method = req
        .method()
        .as_ref()
        .parse::<http::Method>()
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    // absolute uri is acceptable
    let uri = req.url()?.as_str().to_string();

    // base http request
    let mut base: HttpRequest<()> = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .body(())
        .map_err(|e| worker::Error::RustError(format!("build request failed: {e}")))?;

    // copy headers
    let hmap: http::HeaderMap = req.headers().into();
    *base.headers_mut() = hmap;

    // body
    let body_bytes = req.bytes().await.unwrap_or_default();
    let body = if body_bytes.is_empty() {
        ReqBody::Empty
    } else {
        ReqBody::Once(Bytes::from(body_bytes))
    };

    let (parts, _) = base.into_parts();
    Ok(SRequest::from_parts(parts, body))
}

async fn to_worker_response(res: &mut SResponse) -> worker::Result<WResponse> {
    let status = res.status().as_u16();
    let headers = WHeaders::from(res.headers().clone());
    let body = res.take_body();
    let bytes = match body {
        ResBody::None => Vec::new(),
        ResBody::Once(b) => b.to_vec(),
        other => other
            .collect()
            .await
            .map_err(|e| worker::Error::RustError(format!("collect body error: {e}")))?
            .to_bytes()
            .to_vec(),
    };

    let mut wres = WResponse::from_bytes(bytes)?;
    wres = wres.with_status(status);
    Ok(wres.with_headers(headers))
}
