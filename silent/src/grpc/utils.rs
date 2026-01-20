use crate::Response;
use crate::prelude::ResBody;
use http::response::Parts;
use http_body_util::BodyExt;
use tonic::body::Body;

#[cfg(feature = "grpc")]
/// 合并axum响应
#[inline]
pub async fn merge_grpc_response(res: &mut Response, grpc_res: http::Response<Body>) {
    let (parts, body) = grpc_res.into_parts();
    let Parts {
        status,
        headers,
        extensions,
        version,
        ..
    } = parts;
    res.status = status;
    res.version = version;
    res.headers.extend(headers);
    res.extensions.extend(extensions);
    res.body = ResBody::Boxed(Box::pin(body.map_err(|e| e.into())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    // ==================== 基本功能测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_basic() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.status.as_u16(), 200);
    }

    #[tokio::test]
    async fn test_merge_grpc_response_with_body() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        // 验证 body 被设置
        assert!(matches!(res.body, ResBody::Boxed(_)));
    }

    #[tokio::test]
    async fn test_merge_grpc_response_empty_body() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.status.as_u16(), 200);
    }

    // ==================== 状态码测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_status_200() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_merge_grpc_response_status_404() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_merge_grpc_response_status_500() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ==================== Headers 测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_with_headers() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-status", "0")
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.headers.get("content-type").unwrap(), "application/grpc");
        assert_eq!(res.headers.get("grpc-status").unwrap(), "0");
    }

    #[tokio::test]
    async fn test_merge_grpc_response_headers_extend() {
        let mut res = Response::empty();
        res.headers_mut()
            .insert("x-custom", "existing".parse().unwrap());

        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .header("x-new", "new-header".to_string())
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.headers.get("x-custom").unwrap(), "existing");
        assert_eq!(res.headers.get("x-new").unwrap(), "new-header");
    }

    #[tokio::test]
    async fn test_merge_grpc_response_multiple_headers() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "gzip")
            .header("grpc-status", "0")
            .header("grpc-message", "OK")
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.headers.get("content-type").unwrap(), "application/grpc");
        assert_eq!(res.headers.get("grpc-encoding").unwrap(), "gzip");
        assert_eq!(res.headers.get("grpc-status").unwrap(), "0");
        assert_eq!(res.headers.get("grpc-message").unwrap(), "OK");
    }

    // ==================== Version 测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_http_2() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .version(http::Version::HTTP_2)
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.version, http::Version::HTTP_2);
    }

    #[tokio::test]
    async fn test_merge_grpc_response_http_11() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .version(http::Version::HTTP_11)
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.version, http::Version::HTTP_11);
    }

    // ==================== Extensions 测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_with_extensions() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();

        let mut grpc_res_builder = http::Response::builder().status(StatusCode::OK);
        if let Some(ext) = grpc_res_builder.extensions_mut() {
            ext.insert("test_extension");
        }
        let grpc_res = grpc_res_builder.body(grpc_body).unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(
            res.extensions.get::<&'static str>(),
            Some(&"test_extension")
        );
    }

    #[tokio::test]
    async fn test_merge_grpc_response_extensions_extend() {
        let mut res = Response::empty();
        res.extensions_mut().insert("existing_extension");

        let grpc_body = Body::empty();
        let mut grpc_res_builder = http::Response::builder().status(StatusCode::OK);
        if let Some(ext) = grpc_res_builder.extensions_mut() {
            ext.insert("new_extension");
        }
        let grpc_res = grpc_res_builder.body(grpc_body).unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        // 同类型的扩展会被替换为新的值
        assert_eq!(res.extensions.get::<&'static str>(), Some(&"new_extension"));
        // 验证扩展确实存在
        assert!(res.extensions.get::<&'static str>().is_some());
    }

    // ==================== Body 测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_body_is_stream() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        // 验证 body 是 Boxed 类型（流式 body）
        assert!(matches!(res.body, ResBody::Boxed(_)));
    }

    // ==================== 组合测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_full_response() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();

        let mut grpc_res_builder = http::Response::builder()
            .status(StatusCode::OK)
            .version(http::Version::HTTP_2)
            .header("content-type", "application/grpc")
            .header("grpc-status", "0");
        if let Some(ext) = grpc_res_builder.extensions_mut() {
            ext.insert(42i32);
        }
        let grpc_res = grpc_res_builder.body(grpc_body).unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        assert_eq!(res.status, StatusCode::OK);
        assert_eq!(res.version, http::Version::HTTP_2);
        assert_eq!(res.headers.get("content-type").unwrap(), "application/grpc");
        assert_eq!(res.headers.get("grpc-status").unwrap(), "0");
        assert_eq!(res.extensions.get::<i32>(), Some(&42));
        assert!(matches!(res.body, ResBody::Boxed(_)));
    }

    // ==================== 边界条件测试 ====================

    #[tokio::test]
    async fn test_merge_grpc_response_no_headers() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        // 应该成功，即使没有 headers
        assert_eq!(res.status.as_u16(), 200);
    }

    #[tokio::test]
    async fn test_merge_grpc_response_no_extensions() {
        let mut res = Response::empty();
        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        // 应该成功，即使没有 extensions
        assert_eq!(res.status.as_u16(), 200);
    }

    #[tokio::test]
    async fn test_merge_grpc_response_preserves_existing_data() {
        let mut res = Response::empty();
        res.headers_mut()
            .insert("x-existing", "keep".parse().unwrap());
        res.extensions_mut().insert("existing");

        let grpc_body = Body::empty();
        let grpc_res = http::Response::builder()
            .status(StatusCode::OK)
            .body(grpc_body)
            .unwrap();

        merge_grpc_response(&mut res, grpc_res).await;

        // 现有数据应该被保留
        assert_eq!(res.headers.get("x-existing").unwrap(), "keep");
        assert_eq!(res.extensions.get::<&'static str>(), Some(&"existing"));
    }
}
