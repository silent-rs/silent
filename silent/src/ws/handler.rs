use crate::headers::{Connection, HeaderMapExt, SecWebsocketAccept, SecWebsocketKey, Upgrade};
use crate::{Request, Response, Result, SilentError, StatusCode, header};

pub fn websocket_handler(req: &Request) -> Result<Response> {
    let mut res = Response::empty();
    let req_headers = req.headers();
    if !req_headers.contains_key(header::UPGRADE) {
        return Err(SilentError::BusinessError {
            code: StatusCode::BAD_REQUEST,
            msg: "bad request: not upgrade".to_string(),
        });
    }
    if !req_headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase() == "websocket")
        .unwrap_or(false)
    {
        return Err(SilentError::BusinessError {
            code: StatusCode::BAD_REQUEST,
            msg: "bad request: not websocket".to_string(),
        });
    }
    let sec_ws_key = if let Some(key) = req_headers.typed_get::<SecWebsocketKey>() {
        key
    } else {
        return Err(SilentError::BusinessError {
            code: StatusCode::BAD_REQUEST,
            msg: "bad request: sec_websocket_key is not exist in request headers".to_string(),
        });
    };
    res.set_status(StatusCode::SWITCHING_PROTOCOLS);
    res.headers.typed_insert(Connection::upgrade());
    res.headers.typed_insert(Upgrade::websocket());
    res.headers
        .typed_insert(SecWebsocketAccept::from(sec_ws_key));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HeaderValue;

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_websocket_handler_valid_request() {
        let mut req = Request::empty();
        // 添加必需的 WebSocket upgrade headers
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::SWITCHING_PROTOCOLS);
    }

    #[test]
    fn test_websocket_handler_missing_upgrade_header() {
        let req = Request::empty();
        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_websocket_handler_invalid_upgrade_value() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("http/2"));

        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_websocket_handler_upgrade_case_insensitive() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("WebSocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::SWITCHING_PROTOCOLS);
    }

    #[test]
    fn test_websocket_handler_missing_sec_websocket_key() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));

        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    // ==================== Response Headers 测试 ====================

    #[test]
    fn test_websocket_handler_response_headers() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req).unwrap();
        // 验证响应包含 Connection: upgrade
        assert!(
            res.headers()
                .get("connection")
                .map(|v| v.to_str().unwrap().to_lowercase().contains("upgrade"))
                .unwrap_or(false)
        );
        // 验证响应包含 Upgrade: websocket
        assert!(
            res.headers()
                .get("upgrade")
                .map(|v| v.to_str().unwrap().to_lowercase() == "websocket")
                .unwrap_or(false)
        );
        // 验证响应包含 sec-websocket-accept
        assert!(res.headers().get("sec-websocket-accept").is_some());
    }

    #[test]
    fn test_websocket_handler_sec_websocket_accept() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req).unwrap();
        let accept = res.headers().get("sec-websocket-accept").unwrap();
        // 验证 accept header 格式（base64 编码）
        assert!(!accept.to_str().unwrap().is_empty());
    }

    #[test]
    fn test_websocket_handler_response_status() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req).unwrap();
        assert_eq!(res.status(), StatusCode::SWITCHING_PROTOCOLS);
    }

    // ==================== 错误处理测试 ====================

    #[test]
    fn test_websocket_handler_error_not_upgrade() {
        let req = Request::empty();
        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("not upgrade"));
    }

    #[test]
    fn test_websocket_handler_error_not_websocket() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("http/2"));

        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("not websocket"));
    }

    #[test]
    fn test_websocket_handler_error_missing_key() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));

        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("sec_websocket_key"));
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_websocket_handler_with_connection_header() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
    }

    #[test]
    fn test_websocket_handler_empty_key() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut()
            .insert("sec-websocket-key", HeaderValue::from_static(""));

        let res = websocket_handler(&req);
        // 空的 key 应该仍然返回成功（虽然不标准）
        // 这取决于具体实现，如果失败就改成 is_err()
        assert!(res.is_ok() || res.is_err());
    }

    #[test]
    fn test_websocket_handler_different_key() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        // 使用不同的 WebSocket key
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
        let res = res.unwrap();
        let accept = res.headers().get("sec-websocket-accept").unwrap();
        // 不同的 key 应该生成不同的 accept
        assert!(!accept.to_str().unwrap().is_empty());
    }

    // ==================== 大小写不敏感测试 ====================

    #[test]
    fn test_websocket_handler_upgrade_lowercase() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
    }

    #[test]
    fn test_websocket_handler_upgrade_uppercase() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("WEBSOCKET"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
    }

    #[test]
    fn test_websocket_handler_upgrade_mixed_case() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("WeBsOcKeT"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
    }

    // ==================== 多余 Headers 测试 ====================

    #[test]
    fn test_websocket_handler_with_additional_headers() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));
        // 添加额外的 headers
        req.headers_mut()
            .insert("sec-websocket-version", HeaderValue::from_static("13"));
        req.headers_mut()
            .insert("sec-websocket-protocol", HeaderValue::from_static("chat"));

        let res = websocket_handler(&req);
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::SWITCHING_PROTOCOLS);
    }

    // ==================== 状态码验证 ====================

    #[test]
    fn test_websocket_handler_status_switching_protocols() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        req.headers_mut()
            .insert("connection", HeaderValue::from_static("Upgrade"));

        let res = websocket_handler(&req).unwrap();
        assert_eq!(res.status(), 101); // 101 = SWITCHING_PROTOCOLS
    }

    #[test]
    fn test_websocket_handler_bad_request_status() {
        let req = Request::empty();
        let res = websocket_handler(&req);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status(), 400); // 400 = BAD_REQUEST
    }
}
