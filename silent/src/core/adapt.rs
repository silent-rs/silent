#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
#[cfg(feature = "cookie")]
use http::{StatusCode, header};
use http_body::Body;
use hyper::Request as HyperRequest;
use hyper::Response as HyperResponse;

#[cfg(feature = "cookie")]
use crate::CookieExt;
#[cfg(feature = "cookie")]
use crate::SilentError;
use crate::core::req_body::ReqBody;
use crate::{Request, Response};

pub trait RequestAdapt {
    fn tran_to_request(self) -> Request;
}

pub trait ResponseAdapt {
    type Body: Body;
    fn tran_from_response(res: Response<Self::Body>) -> Self;
}

#[allow(clippy::result_large_err)]
#[cfg(feature = "cookie")]
fn get_cookie(req: &HyperRequest<ReqBody>) -> Result<CookieJar, SilentError> {
    let mut jar = CookieJar::new();
    if let Some(cookies) = req.headers().get(header::COOKIE) {
        for cookie_str in cookies
            .to_str()
            .map_err(|e| {
                SilentError::business_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to parse cookie: {e}"),
                )
            })?
            .split(';')
            .map(|s| s.trim())
        {
            if let Ok(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()) {
                jar.add_original(cookie);
            }
        }
    }
    Ok(jar)
}

impl RequestAdapt for HyperRequest<ReqBody> {
    fn tran_to_request(self) -> Request {
        #[cfg(feature = "cookie")]
        let cookies = get_cookie(&self).unwrap_or_default();
        let (parts, body) = self.into_parts();
        #[allow(unused_mut)]
        let mut req = Request::from_parts(parts, body);
        #[cfg(feature = "cookie")]
        req.extensions_mut().insert(cookies);
        req
    }
}

impl<B: Body> ResponseAdapt for HyperResponse<B> {
    type Body = B;
    fn tran_from_response(res: Response<B>) -> Self {
        #[cfg(feature = "cookie")]
        let cookies = res.cookies();
        let Response {
            status,
            headers,
            body,
            version,
            extensions,
            ..
        } = res;

        let mut res = hyper::Response::new(body);
        res.headers_mut().extend(headers);
        #[cfg(feature = "cookie")]
        for cookie in cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse() {
                res.headers_mut().append(header::SET_COOKIE, hv);
            }
        }
        res.extensions_mut().extend(extensions);
        *res.version_mut() = version;
        *res.status_mut() = status;

        res
    }
}
