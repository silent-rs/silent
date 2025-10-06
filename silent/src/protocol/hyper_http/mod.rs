use hyper::{Request as HyperRequest, Response as HyperResponse};

#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
#[cfg(feature = "cookie")]
use http::{StatusCode, header};

use crate::core::req_body::ReqBody;
use crate::core::res_body::ResBody;
use crate::protocol::Protocol;
use crate::{Request, Response};

#[cfg(feature = "cookie")]
use crate::CookieExt;
#[cfg(feature = "cookie")]
use crate::SilentError;

/// hyper HTTP 协议适配器
pub struct HyperHttpProtocol;

impl Protocol for HyperHttpProtocol {
    type Incoming = HyperRequest<ReqBody>;
    type Outgoing = HyperResponse<ResBody>;
    type Body = ResBody;
    type InternalRequest = Request;
    type InternalResponse = Response<Self::Body>;

    fn into_internal(message: Self::Incoming) -> Self::InternalRequest {
        #[cfg(feature = "cookie")]
        let cookies = get_cookie(&message).unwrap_or_default();
        let (parts, body) = message.into_parts();
        #[allow(unused_mut)]
        let mut request = Request::from_parts(parts, body);
        #[cfg(feature = "cookie")]
        request.extensions_mut().insert(cookies);
        request
    }

    fn from_internal(response: Self::InternalResponse) -> Self::Outgoing {
        #[cfg(feature = "cookie")]
        let cookies = response.cookies();
        let Response {
            status,
            headers,
            body,
            version,
            extensions,
            ..
        } = response;

        let mut response = HyperResponse::new(body);
        response.headers_mut().extend(headers);
        #[cfg(feature = "cookie")]
        for cookie in cookies.delta() {
            if let Ok(header_value) = cookie.encoded().to_string().parse() {
                response
                    .headers_mut()
                    .append(header::SET_COOKIE, header_value);
            }
        }
        response.extensions_mut().extend(extensions);
        *response.version_mut() = version;
        *response.status_mut() = status;

        response
    }
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
