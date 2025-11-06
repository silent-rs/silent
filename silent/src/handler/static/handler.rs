use std::borrow::Cow;
use std::path::PathBuf;
use std::str::FromStr;

use async_compression::futures::bufread::{BrotliEncoder, GzipEncoder};
use async_fs::{File, metadata};
use async_trait::async_trait;
use bytes::Bytes;
use futures::io::{AsyncRead, AsyncReadExt, BufReader};
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use headers::ContentType;
use http::header::CONTENT_LENGTH;
use mime::CHARSET;

use crate::prelude::stream_body;
use crate::{Handler, Request, Response, SilentError, StatusCode};

use super::StaticOptions;
use super::compression::{Compression, apply_headers, negotiate};
use super::directory::render_directory_listing;

pub struct HandlerWrapperStatic {
    path: String,
    options: StaticOptions,
}

impl HandlerWrapperStatic {
    fn new(path: &str, options: StaticOptions) -> Self {
        let mut normalized = path;
        if normalized.ends_with('/') {
            normalized = &normalized[..normalized.len() - 1];
        }
        if !std::path::Path::new(normalized).is_dir() {
            panic!("Path not exists: {normalized}");
        }
        Self {
            path: normalized.to_string(),
            options,
        }
    }

    fn decode_param(param: &str) -> Result<String, SilentError> {
        urlencoding::decode(param)
            .map(Cow::into_owned)
            .map_err(|_| SilentError::BusinessError {
                code: StatusCode::NOT_FOUND,
                msg: "Not Found".to_string(),
            })
    }
}

#[async_trait]
impl Handler for HandlerWrapperStatic {
    async fn call(&self, req: Request) -> Result<Response, SilentError> {
        if let Ok(file_path) = req.get_path_params::<String>("path") {
            let decoded = Self::decode_param(&file_path)?;
            let ends_with_slash = decoded.ends_with('/') || decoded.is_empty();
            let trimmed = decoded.trim_start_matches('/');

            let mut fs_path = PathBuf::from(&self.path);
            if !trimmed.is_empty() {
                fs_path = fs_path.join(trimmed);
            }

            let meta = metadata(&fs_path).await.ok();
            if self.options.directory_listing {
                let is_dir = ends_with_slash || meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                if is_dir {
                    return render_directory_listing(trimmed, fs_path.as_path()).await;
                }
            }

            let mut target_path = fs_path.clone();
            if ends_with_slash || meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
                target_path = target_path.join("index.html");
            }

            if let Ok(file) = File::open(&target_path).await {
                let mut res = Response::empty();
                let guessed_mime = mime_guess::from_path(&target_path).first();
                res.set_typed_header(normalize_content_type(guessed_mime.clone()));

                let stream =
                    if let Some(kind) = negotiate(&self.options, &req, guessed_mime.as_ref()) {
                        apply_headers(&mut res, &kind);
                        match kind {
                            Compression::Brotli => {
                                let reader = BufReader::new(file);
                                to_stream(BrotliEncoder::new(reader))
                            }
                            Compression::Gzip => {
                                let reader = BufReader::new(file);
                                to_stream(GzipEncoder::new(reader))
                            }
                        }
                    } else {
                        to_stream(file)
                    };

                res.headers_mut().remove(CONTENT_LENGTH);
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

fn to_stream<R>(reader: R) -> BoxStream<'static, Result<Bytes, std::io::Error>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    const CHUNK_SIZE: usize = 16 * 1024;
    let buf = vec![0u8; CHUNK_SIZE];
    stream::try_unfold((reader, buf), |(mut reader, mut buf)| async move {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            Ok(None)
        } else {
            let bytes = Bytes::copy_from_slice(&buf[..n]);
            Ok(Some((bytes, (reader, buf))))
        }
    })
    .boxed()
}

fn normalize_content_type(mime: Option<mime::Mime>) -> ContentType {
    match mime {
        Some(value) => {
            if value.type_() == mime::TEXT && value.get_param(CHARSET).is_none() {
                let raw = format!("{}/{}; charset=utf-8", value.type_(), value.subtype());
                if let Ok(parsed) = mime::Mime::from_str(&raw) {
                    ContentType::from(parsed)
                } else {
                    ContentType::text_utf8()
                }
            } else {
                ContentType::from(value)
            }
        }
        None => ContentType::octet_stream(),
    }
}

pub fn static_handler(path: &str) -> impl Handler {
    HandlerWrapperStatic::new(path, StaticOptions::default())
}

pub fn static_handler_with_options(path: &str, options: StaticOptions) -> impl Handler {
    HandlerWrapperStatic::new(path, options)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE};
    use http_body_util::BodyExt;

    use crate::core::path_param::PathString;
    use crate::prelude::*;
    use crate::{Handler, Request, SilentError, StatusCode};

    use super::{HandlerWrapperStatic, StaticOptions};

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
    impl PathParam {
        #[cfg(test)]
        pub(crate) fn path_owned(value: String) -> Self {
            PathParam::Path(PathString::Owned(value))
        }
    }

    fn create_static(path: &str) {
        if !std::path::Path::new(path).is_dir() {
            std::fs::create_dir(path).unwrap();
            std::fs::write(format!("./{path}/index.html"), CONTENT).unwrap();
            std::fs::write(format!("./{path}/hello.txt"), "hello").unwrap();
            std::fs::create_dir(format!("./{path}/docs")).unwrap();
            std::fs::write(format!("./{path}/docs/readme.txt"), "doc").unwrap();
        }
    }

    fn clean_static(path: &str) {
        if std::path::Path::new(path).is_dir() {
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[tokio::test]
    async fn test_static() {
        let path = "test_static";
        create_static(path);
        let handler = HandlerWrapperStatic::new(path, StaticOptions::default());
        let mut req = Request::default();
        req.set_path_params(
            "path".to_owned(),
            PathParam::path_owned("index.html".to_string()),
        );
        let mut res = handler.call(req).await.unwrap();
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
        let handler = HandlerWrapperStatic::new(path, StaticOptions::default());
        let mut req = Request::default();
        req.set_path_params("path".to_owned(), PathParam::path_owned(String::new()));
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
        let handler = HandlerWrapperStatic::new(path, StaticOptions::default());
        let mut req = Request::default();
        req.set_path_params(
            "path".to_owned(),
            PathParam::path_owned("not_found.html".to_string()),
        );
        let res = handler.call(req).await.unwrap_err();
        clean_static(path);
        if let SilentError::BusinessError { code, .. } = res {
            assert_eq!(code, StatusCode::NOT_FOUND);
        } else {
            panic!();
        }
    }

    #[tokio::test]
    async fn test_directory_listing() {
        let path = "test_static_listing";
        create_static(path);
        let options = StaticOptions::default().with_directory_listing();
        let handler = HandlerWrapperStatic::new(path, options);
        let mut req = Request::default();
        req.set_path_params("path".to_owned(), PathParam::path_owned(String::new()));
        let mut res = handler.call(req).await.unwrap();
        let body = res
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .data_ref()
            .unwrap()
            .clone();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        clean_static(path);
        assert!(body_str.contains("hello.txt"));
        assert!(body_str.contains("./"));
        assert!(!body_str.contains(">../<"));
    }

    #[tokio::test]
    async fn test_compression_negotiation() {
        let path = "test_static_compress";
        create_static(path);
        let options = StaticOptions::default().with_compression();
        let handler = HandlerWrapperStatic::new(path, options);
        let mut req = Request::default();
        req.headers_mut()
            .insert(ACCEPT_ENCODING, "gzip".parse().unwrap());
        req.set_path_params(
            "path".to_owned(),
            PathParam::path_owned("hello.txt".to_string()),
        );
        let res = handler.call(req).await.unwrap();
        clean_static(path);
        assert_eq!(
            res.headers()
                .get(CONTENT_ENCODING)
                .unwrap()
                .to_str()
                .unwrap(),
            "gzip"
        );
    }

    #[tokio::test]
    async fn test_directory_listing_subdir_has_parent_link() {
        let path = "test_static_listing_subdir";
        create_static(path);
        let options = StaticOptions::default().with_directory_listing();
        let handler = HandlerWrapperStatic::new(path, options);
        let mut req = Request::default();
        req.set_path_params(
            "path".to_owned(),
            PathParam::path_owned("docs/".to_string()),
        );
        let mut res = handler.call(req).await.unwrap();
        let body = res
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .data_ref()
            .unwrap()
            .clone();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        clean_static(path);
        assert!(body_str.contains(">../<"));
    }

    #[tokio::test]
    async fn test_text_content_type_uses_utf8() {
        let path = "test_static_text_utf8";
        create_static(path);
        let handler = HandlerWrapperStatic::new(path, StaticOptions::default());
        let mut req = Request::default();
        req.set_path_params(
            "path".to_owned(),
            PathParam::path_owned("hello.txt".to_string()),
        );
        let res = handler.call(req).await.unwrap();
        clean_static(path);
        let header = res.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(header.contains("charset=utf-8"));
    }
}
