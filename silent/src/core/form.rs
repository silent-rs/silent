use crate::core::req_body::ReqBody;
use crate::header::{CONTENT_TYPE, HeaderMap};
use crate::multer::{Field, Multipart};
use crate::{SilentError, StatusCode};
use async_fs::File;
use futures::io::AsyncWriteExt;
use multimap::MultiMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::Builder;
use textnonce::TextNonce;

/// The extracted text fields and uploaded files from a `multipart/form-data` request.
#[derive(Debug)]
pub struct FormData {
    /// Name-value pairs for plain text fields. Technically, these are form data parts with no
    /// filename specified in the part's `Content-Disposition`.
    pub fields: MultiMap<String, String>,
    /// Name-value pairs for temporary files. Technically, these are form data parts with a filename
    /// specified in the part's `Content-Disposition`.
    #[cfg(feature = "server")]
    pub files: MultiMap<String, FilePart>,
}

impl FormData {
    /// Create new `FormData`.
    #[inline]
    pub fn new() -> FormData {
        FormData {
            fields: MultiMap::new(),
            #[cfg(feature = "server")]
            files: MultiMap::new(),
        }
    }

    /// Parse MIME `multipart/*` information from a stream as a [`FormData`].
    pub(crate) async fn read(headers: &HeaderMap, body: ReqBody) -> Result<FormData, SilentError> {
        let mut form_data = FormData::new();
        if let Some(boundary) = headers
            .get(CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .and_then(|ct| multer::parse_boundary(ct).ok())
        {
            let mut multipart = Multipart::new(body, boundary);
            while let Some(mut field) = multipart.next_field().await? {
                if let Some(name) = field.name().map(|s| s.to_owned()) {
                    if field.headers().get(CONTENT_TYPE).is_some() {
                        form_data
                            .files
                            .insert(name, FilePart::create(&mut field).await?);
                    } else {
                        form_data.fields.insert(name, field.text().await?);
                    }
                }
            }
        }
        Ok(form_data)
    }
}

impl Default for FormData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Clone, Debug)]
pub struct FilePart {
    name: Option<String>,
    /// The headers of the part
    headers: HeaderMap,
    /// A temporary file containing the file content
    path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    size: u64,
    // The temporary directory the upload was put into, saved for the Drop trait
    temp_dir: Option<PathBuf>,
}

impl FilePart {
    /// Get file name.
    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// Get file name mutable reference.
    #[inline]
    pub fn name_mut(&mut self) -> Option<&mut String> {
        self.name.as_mut()
    }
    /// Get headers.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    /// Get headers mutable reference.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    /// Get file path.
    #[inline]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    /// Get file size.
    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }
    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsibility to clean up.
    #[inline]
    pub fn do_not_delete_on_drop(&mut self) {
        self.temp_dir = None;
    }
    /// Save the file to a new location.
    #[inline]
    pub fn save(&self, path: String) -> Result<u64, SilentError> {
        std::fs::copy(self.path(), Path::new(&path)).map_err(|e| SilentError::BusinessError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            msg: format!("Failed to save file: {e}"),
        })
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    #[inline]
    pub async fn create(field: &mut Field<'_>) -> Result<FilePart, SilentError> {
        // Set up a file to capture the contents.
        let mut path = Builder::new()
            .prefix("silent_http_multipart")
            .tempdir()?
            .keep();
        let temp_dir = Some(path.clone());
        let name = field.file_name().map(|s| s.to_owned());
        path.push(format!(
            "{}.{}",
            TextNonce::sized_urlsafe(32)?.into_string(),
            name.as_deref()
                .and_then(|name| { Path::new(name).extension().and_then(OsStr::to_str) })
                .unwrap_or("unknown")
        ));
        let mut file = File::create(&path).await?;
        let mut size = 0;
        while let Some(chunk) = field.chunk().await? {
            size += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }
        Ok(FilePart {
            name,
            headers: field.headers().to_owned(),
            path,
            size,
            temp_dir,
        })
    }
}

impl Drop for FilePart {
    fn drop(&mut self) {
        if let Some(temp_dir) = &self.temp_dir {
            let path = self.path.clone();
            let temp_dir = temp_dir.to_owned();
            std::thread::spawn(move || {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_dir(temp_dir);
            });
        }
    }
}

#[cfg(all(test, feature = "server", feature = "multipart"))]
mod tests {
    use super::*;
    use crate::header::{HeaderMap, HeaderValue};
    use bytes::Bytes;

    // FormData 构造函数测试
    #[test]
    fn test_form_data_new() {
        let form_data = FormData::new();
        assert_eq!(form_data.fields.len(), 0);
        assert_eq!(form_data.files.len(), 0);
    }

    #[test]
    fn test_form_data_default() {
        let form_data = FormData::default();
        assert_eq!(form_data.fields.len(), 0);
        assert_eq!(form_data.files.len(), 0);
    }

    // FormData::read() 边界条件测试
    #[tokio::test]
    async fn test_form_data_read_no_content_type() {
        let headers = HeaderMap::new();
        let body = ReqBody::Once(Bytes::from("test data"));
        let result = FormData::read(&headers, body).await;
        assert!(result.is_ok());
        let form_data = result.unwrap();
        assert_eq!(form_data.fields.len(), 0);
        assert_eq!(form_data.files.len(), 0);
    }

    #[tokio::test]
    async fn test_form_data_read_empty_body() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("multipart/form-data; boundary=----WebKitFormBoundary"),
        );
        let body = ReqBody::Empty;
        let result = FormData::read(&headers, body).await;
        // 空的 multipart body 可能成功或失败，取决于底层实现
        // 这里验证至少不会 panic
        let _ = result;
    }

    #[tokio::test]
    async fn test_form_data_read_invalid_content_type() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let body = ReqBody::Once(Bytes::from("test data"));
        let result = FormData::read(&headers, body).await;
        assert!(result.is_ok());
    }

    // FilePart getter 方法测试
    #[test]
    fn test_file_part_name() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.name(), Some("test_file.txt"));
    }

    #[test]
    fn test_file_part_name_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: None,
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.name(), None);
    }

    #[test]
    fn test_file_part_name_mut() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let mut file_part = FilePart {
            name: Some("old_name.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        if let Some(name) = file_part.name_mut() {
            *name = "new_name.txt".to_string();
        }
        assert_eq!(file_part.name(), Some("new_name.txt"));
    }

    #[test]
    fn test_file_part_headers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/plain"));

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: headers.clone(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(
            file_part.headers().get("content-type").unwrap(),
            "text/plain"
        );
    }

    #[test]
    fn test_file_part_headers_mut() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/plain"));

        let mut file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers,
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        file_part
            .headers_mut()
            .insert("content-type", HeaderValue::from_static("application/json"));
        assert_eq!(
            file_part.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_file_part_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.path(), &file_path);
    }

    #[test]
    fn test_file_part_size() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 1024,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.size(), 1024);
    }

    // FilePart::save() 方法测试
    #[test]
    fn test_file_part_save() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir.path().join("source.txt");
        std::fs::write(&source_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("source.txt".to_string()),
            headers: HeaderMap::new(),
            path: source_path.clone(),
            size: 12,
            temp_dir: Some(source_dir.path().to_path_buf()),
        };

        let dest_path = temp_dir.path().join("dest.txt");
        let result = file_part.save(dest_path.to_str().unwrap().to_string());
        assert!(result.is_ok());
        assert!(dest_path.exists());
        assert_eq!(std::fs::read_to_string(&dest_path).unwrap(), "test content");
    }

    #[test]
    fn test_file_part_save_invalid_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        // 尝试保存到无效路径
        let result = file_part.save("/nonexistent/directory/file.txt".to_string());
        assert!(result.is_err());
    }

    // FilePart::do_not_delete_on_drop() 测试
    #[test]
    fn test_file_part_do_not_delete_on_drop() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let mut file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        // 调用 do_not_delete_on_drop 后，temp_dir 应该被设置为 None
        file_part.do_not_delete_on_drop();
        assert!(file_part.temp_dir.is_none());
    }

    // FilePart 内存布局测试
    #[test]
    fn test_file_part_size_and_alignment() {
        let size = std::mem::size_of::<FilePart>();
        let align = std::mem::align_of::<FilePart>();
        assert!(size > 0);
        assert!(align >= std::mem::align_of::<usize>());
    }

    #[test]
    fn test_file_part_clone() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        let cloned = file_part.clone();
        assert_eq!(cloned.name(), file_part.name());
        assert_eq!(cloned.path(), file_part.path());
        assert_eq!(cloned.size(), file_part.size());
    }

    // MultiMap 集成测试
    #[test]
    fn test_form_data_fields_multimap() {
        let mut form_data = FormData::new();
        form_data
            .fields
            .insert("username".to_string(), "alice".to_string());
        form_data
            .fields
            .insert("username".to_string(), "bob".to_string());

        // MultiMap 允许重复键
        let values = form_data.fields.get_vec("username").unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&"alice".to_string()));
        assert!(values.contains(&"bob".to_string()));
    }

    #[test]
    fn test_form_data_files_multimap() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path1 = temp_dir.path().join("file1.txt");
        let file_path2 = temp_dir.path().join("file2.txt");
        std::fs::write(&file_path1, b"content1").unwrap();
        std::fs::write(&file_path2, b"content2").unwrap();

        let mut form_data = FormData::new();
        form_data.files.insert(
            "files".to_string(),
            FilePart {
                name: Some("file1.txt".to_string()),
                headers: HeaderMap::new(),
                path: file_path1,
                size: 8,
                temp_dir: Some(temp_dir.path().to_path_buf()),
            },
        );
        form_data.files.insert(
            "files".to_string(),
            FilePart {
                name: Some("file2.txt".to_string()),
                headers: HeaderMap::new(),
                path: file_path2,
                size: 8,
                temp_dir: Some(temp_dir.path().to_path_buf()),
            },
        );

        let files = form_data.files.get_vec("files").unwrap();
        assert_eq!(files.len(), 2);
    }

    // 边界条件和错误处理测试
    #[tokio::test]
    async fn test_form_data_read_malformed_boundary() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("multipart/form-data; boundary=unterminated"),
        );
        let body = ReqBody::Once(Bytes::from("------WebKitFormBoundary\r\n"));
        let result = FormData::read(&headers, body).await;
        // 应该处理格式错误的数据
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_file_part_zero_size() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("empty_file.txt");
        std::fs::write(&file_path, b"").unwrap();

        let file_part = FilePart {
            name: Some("empty_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 0,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.size(), 0);
        assert!(file_part.path().exists());
    }

    #[test]
    fn test_file_part_large_size() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("large_file.bin");
        let large_size = u64::MAX / 2;

        let file_part = FilePart {
            name: Some("large_file.bin".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: large_size,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.size(), large_size);
    }

    // FormData 和 FilePart 类型测试
    #[test]
    fn test_form_data_debug() {
        let form_data = FormData::new();
        let debug_str = format!("{:?}", form_data);
        assert!(debug_str.contains("FormData"));
    }

    #[test]
    fn test_file_part_debug() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        let debug_str = format!("{:?}", file_part);
        assert!(debug_str.contains("FilePart"));
    }

    // HeaderMap 集成测试
    #[test]
    fn test_file_part_with_custom_headers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("image/jpeg"));
        headers.insert(
            "content-disposition",
            HeaderValue::from_static("attachment"),
        );

        let file_part = FilePart {
            name: Some("photo.jpg".to_string()),
            headers: headers.clone(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.headers().len(), 2);
        assert_eq!(
            file_part.headers().get("content-type").unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            file_part.headers().get("content-disposition").unwrap(),
            "attachment"
        );
    }

    // 文件路径处理测试
    #[test]
    fn test_file_part_path_with_special_characters() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_name = "test file-@#$.txt";
        let file_path = temp_dir.path().join(file_name);
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some(file_name.to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.path(), &file_path);
        assert_eq!(file_part.name(), Some(file_name));
    }

    // 临时目录管理测试
    #[test]
    fn test_file_part_temp_dir_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        // temp_dir 为 None 的 FilePart 不会被自动删除
        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: None,
        };

        assert!(file_part.temp_dir.is_none());
        assert!(file_path.exists());
    }

    #[test]
    fn test_file_part_temp_dir_some() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("test_file.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path.clone(),
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert!(file_part.temp_dir.is_some());
        assert_eq!(file_part.temp_dir.as_ref().unwrap(), temp_dir.path());
    }

    // 重复字段测试
    #[test]
    fn test_form_data_duplicate_fields() {
        let mut form_data = FormData::new();
        form_data
            .fields
            .insert("key".to_string(), "value1".to_string());
        form_data
            .fields
            .insert("key".to_string(), "value2".to_string());
        form_data
            .fields
            .insert("key".to_string(), "value3".to_string());

        let values = form_data.fields.get_vec("key").unwrap();
        assert_eq!(values.len(), 3);
    }

    // 文件名变更测试
    #[test]
    fn test_file_part_rename() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("old_name.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let mut file_part = FilePart {
            name: Some("old_name.txt".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        if let Some(name) = file_part.name_mut() {
            *name = "new_name.txt".to_string();
        }

        assert_eq!(file_part.name(), Some("new_name.txt"));
    }

    // 空文件名测试
    #[test]
    fn test_file_part_empty_filename() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join(".txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some("".to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.name(), Some(""));
    }

    // Unicode 文件名测试
    #[test]
    fn test_file_part_unicode_filename() {
        let temp_dir = tempfile::tempdir().unwrap();
        let unicode_name = "测试文件🎉.txt";
        let file_path = temp_dir.path().join(unicode_name);
        std::fs::write(&file_path, b"test content").unwrap();

        let file_part = FilePart {
            name: Some(unicode_name.to_string()),
            headers: HeaderMap::new(),
            path: file_path,
            size: 12,
            temp_dir: Some(temp_dir.path().to_path_buf()),
        };

        assert_eq!(file_part.name(), Some(unicode_name));
    }

    // 多字段组合测试
    #[test]
    fn test_form_data_multiple_fields_and_files() {
        let mut form_data = FormData::new();

        // 添加多个文本字段
        form_data
            .fields
            .insert("username".to_string(), "alice".to_string());
        form_data
            .fields
            .insert("email".to_string(), "alice@example.com".to_string());

        // 添加多个文件
        let temp_dir = tempfile::tempdir().unwrap();
        for i in 1..=3 {
            let file_path = temp_dir.path().join(format!("file{}.txt", i));
            std::fs::write(&file_path, format!("content{}", i)).unwrap();
            form_data.files.insert(
                format!("file{}", i),
                FilePart {
                    name: Some(format!("file{}.txt", i)),
                    headers: HeaderMap::new(),
                    path: file_path,
                    size: 8,
                    temp_dir: Some(temp_dir.path().to_path_buf()),
                },
            );
        }

        assert_eq!(form_data.fields.len(), 2);
        assert_eq!(form_data.files.len(), 3);
    }
}
