use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::prelude::stream_body;
use crate::{Handler, Request, Response, SilentError, StatusCode};

struct HandlerWrapperStatic {
    path: String,
}

impl Default for HandlerWrapperStatic {
    fn default() -> Self {
        Self::new(".")
    }
}

impl HandlerWrapperStatic {
    fn new(path: &str) -> Self {
        let mut path = path;
        if path.ends_with('/') {
            path = &path[..path.len() - 1];
        }
        if !std::path::Path::new(path).is_dir() {
            panic!("Path not exists: {path}");
        }
        Self {
            path: path.to_string(),
        }
    }
}

#[async_trait]
impl Handler for HandlerWrapperStatic {
    async fn call(&self, req: Request) -> Result<Response, SilentError> {
        if let Ok(file_path) = req.get_path_params::<String>("path") {
            // 文件路径使用url解码
            let file_path =
                urlencoding::decode(&file_path).map_err(|_| SilentError::BusinessError {
                    code: StatusCode::NOT_FOUND,
                    msg: "Not Found".to_string(),
                })?;
            let mut path = format!("{}/{}", self.path, file_path);
            if path.ends_with('/') {
                path.push_str("index.html");
            }
            if let Ok(file) = File::open(&path).await {
                let mut res = Response::empty();
                if let Some(content_type) = mime_guess::from_path(path).first() {
                    res.set_typed_header(headers::ContentType::from(content_type));
                } else {
                    res.set_typed_header(headers::ContentType::octet_stream());
                }
                let reader_stream = ReaderStream::new(file);
                let stream = reader_stream.boxed();
                res.set_body(stream_body(stream));
                return Ok(res);
            }
        }
        Err(SilentError::BusinessError {
            code: StatusCode::NOT_FOUND,
            msg: "Not Found".to_string(),
        })
    }
}

pub fn static_handler(path: &str) -> impl Handler {
    HandlerWrapperStatic::new(path)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http_body_util::BodyExt;

    use crate::Handler;
    use crate::Request;
    use crate::SilentError;
    use crate::StatusCode;
    use crate::prelude::*;

    use super::HandlerWrapperStatic;

    static CONTENT: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Silent</title>
</head>
<body>

<h1>我的第一个标题</h1>

<p>我的第一个段落。</p>

</body>
</html>"#;

    fn create_static(path: &str) {
        if !std::path::Path::new(path).is_dir() {
            std::fs::create_dir(path).unwrap();
            std::fs::write(format!("./{path}/index.html"), CONTENT).unwrap();
        }
    }

    fn clean_static(path: &str) {
        if std::path::Path::new(path).is_dir() {
            std::fs::remove_file(format!("./{path}/index.html")).unwrap();
            std::fs::remove_dir(path).unwrap();
        }
    }

    #[tokio::test]
    async fn test_static() {
        let path = "test_static";
        create_static(path);
        let handler = HandlerWrapperStatic::new(path);
        let mut req = Request::default();
        req.set_path_params("path".to_owned(), PathParam::Path("index.html".to_string()));
        let mut res = handler.call(req).await.unwrap();
        clean_static(path);
        assert_eq!(res.status, StatusCode::OK);
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from(CONTENT)
        );
    }

    #[tokio::test]
    async fn test_static_hash_mode() {
        let path = "test_static";
        create_static(path);
        let route = Route::new("test").with_static(path);
        let route = Route::new_root()
            .append(
                Route::new("test").get(async move |_req: Request| Ok("Hello, world!".to_string())),
            )
            .append(route)
            .with_static(path);
        let mut req = Request::default();
        *req.uri_mut() = "/test/#/hello?name=hubert".parse().unwrap();
        let mut res = route.call(req).await.unwrap();
        clean_static(path);
        assert_eq!(res.status, StatusCode::OK);
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from(CONTENT)
        );
    }

    #[tokio::test]
    async fn test_static_default() {
        let path = "test_static_default";
        create_static(path);
        let handler = HandlerWrapperStatic::new(path);
        let mut req = Request::default();
        req.set_path_params("path".to_owned(), PathParam::Path("".to_string()));
        let mut res = handler.call(req).await.unwrap();
        clean_static(path);
        assert_eq!(res.status, StatusCode::OK);
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from(CONTENT)
        );
    }

    #[tokio::test]
    async fn test_static_not_found() {
        let path = "test_static_not_found";
        create_static(path);
        let handler = HandlerWrapperStatic::new(path);
        let mut req = Request::default();
        req.set_path_params(
            "path".to_owned(),
            PathParam::Path("not_found.html".to_string()),
        );
        let res = handler.call(req).await.unwrap_err();
        clean_static(path);
        if let SilentError::BusinessError { code, .. } = res {
            assert_eq!(code, StatusCode::NOT_FOUND);
        } else {
            panic!();
        }
    }
}
