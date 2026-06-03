use std::path::Path;

use async_fs::read_dir;
use futures_util::StreamExt;
use headers::ContentType;

use crate::{Response, SilentError, StatusCode};

pub(super) async fn render_directory_listing(
    relative_path: &str,
    target: &Path,
) -> Result<Response, SilentError> {
    let mut dir = read_dir(target)
        .await
        .map_err(|_| SilentError::BusinessError {
            code: StatusCode::NOT_FOUND,
            msg: "Not Found".to_string(),
        })?;

    let mut entries = Vec::new();
    while let Some(entry_res) = dir.next().await {
        let entry = entry_res.map_err(|err| SilentError::BusinessError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            msg: format!("Read dir failed: {err}"),
        })?;
        let file_type = entry
            .file_type()
            .await
            .map_err(|err| SilentError::BusinessError {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                msg: format!("Read dir entry failed: {err}"),
            })?;
        let name_os = entry.file_name();
        let name = name_os.to_string_lossy();
        let escaped = escape_html(&name);
        let encoded = urlencoding::encode(&name);
        let suffix = if file_type.is_dir() { "/" } else { "" };
        entries.push((escaped, encoded.into_owned(), suffix.to_string()));
    }

    entries.sort_by_key(|a| a.0.to_lowercase());

    let display_path = if relative_path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", relative_path)
    };

    let mut body = String::new();
    body.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Index of ");
    body.push_str(&escape_html(&display_path));
    body.push_str("</title><style>body{font-family:monospace;}a{text-decoration:none;}ul{list-style:none;padding-left:0;}</style></head><body>");
    body.push_str(&format!("<h1>Index of {}</h1>", escape_html(&display_path)));
    body.push_str("<ul>");
    body.push_str("<li><a href=\"./\">./</a></li>");
    if !relative_path.is_empty() {
        body.push_str("<li><a href=\"../\">../</a></li>");
    }
    for (display, href, suffix) in entries {
        body.push_str(&format!(
            "<li><a href=\"./{}{}\">{}{}</a></li>",
            href, suffix, display, suffix
        ));
    }
    body.push_str("</ul></body></html>");

    let mut res = Response::empty();
    res.set_typed_header(ContentType::html());
    res.set_body(body.into());
    Ok(res)
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper function to extract body data from Response
    async fn extract_body(mut res: Response) -> String {
        use http_body_util::BodyExt;

        // Take the body from the response
        let mut body = res.take_body();
        let collected = BodyExt::collect(&mut body).await.unwrap();
        String::from_utf8(collected.to_bytes().to_vec()).unwrap()
    }

    // ==================== escape_html 测试 ====================

    #[test]
    fn test_escape_html_ampersand() {
        assert_eq!(escape_html("&"), "&amp;");
        assert_eq!(escape_html("a&b"), "a&amp;b");
        assert_eq!(escape_html("&&"), "&amp;&amp;");
    }

    #[test]
    fn test_escape_html_less_than() {
        assert_eq!(escape_html("<"), "&lt;");
        assert_eq!(escape_html("a<b"), "a&lt;b");
        assert_eq!(escape_html("<<"), "&lt;&lt;");
    }

    #[test]
    fn test_escape_html_greater_than() {
        assert_eq!(escape_html(">"), "&gt;");
        assert_eq!(escape_html("a>b"), "a&gt;b");
        assert_eq!(escape_html(">>"), "&gt;&gt;");
    }

    #[test]
    fn test_escape_html_double_quote() {
        assert_eq!(escape_html("\""), "&quot;");
        assert_eq!(escape_html("a\"b"), "a&quot;b");
        assert_eq!(escape_html("\"\""), "&quot;&quot;");
    }

    #[test]
    fn test_escape_html_single_quote() {
        assert_eq!(escape_html("'"), "&#39;");
        assert_eq!(escape_html("a'b"), "a&#39;b");
        assert_eq!(escape_html("''"), "&#39;&#39;");
    }

    #[test]
    fn test_escape_html_empty_string() {
        assert_eq!(escape_html(""), "");
    }

    #[test]
    fn test_escape_html_normal_characters() {
        assert_eq!(escape_html("abc123"), "abc123");
        assert_eq!(escape_html("hello world"), "hello world");
    }

    #[test]
    fn test_escape_html_mixed_special_chars() {
        assert_eq!(
            escape_html("<script>alert('XSS')</script>"),
            "&lt;script&gt;alert(&#39;XSS&#39;)&lt;/script&gt;"
        );
        assert_eq!(
            escape_html("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&#39;f"
        );
    }

    #[test]
    fn test_escape_html_unicode() {
        assert_eq!(escape_html("你好"), "你好");
        assert_eq!(escape_html("こんにちは"), "こんにちは");
        assert_eq!(escape_html("<🎉>"), "&lt;🎉&gt;");
    }

    // ==================== render_directory_listing 测试 ====================

    #[tokio::test]
    async fn test_render_directory_listing_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 应该包含 HTML 结构
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("<title>Index of /</title>"));
        assert!(body.contains("<h1>Index of /</h1>"));
        assert!(body.contains("<ul>"));
        assert!(body.contains("</ul>"));

        // 应该包含 ./ 但不包含 ../（根目录）
        assert!(body.contains("<li><a href=\"./\">./</a></li>"));
        assert!(!body.contains("<li><a href=\"../\">../</a></li>"));
    }

    #[tokio::test]
    async fn test_render_directory_listing_with_files() {
        let temp_dir = TempDir::new().unwrap();
        fs::File::create(temp_dir.path().join("file1.txt")).unwrap();
        fs::File::create(temp_dir.path().join("file2.txt")).unwrap();

        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 应该包含文件链接
        assert!(body.contains("<li><a href=\"./file1.txt\">file1.txt</a></li>"));
        assert!(body.contains("<li><a href=\"./file2.txt\">file2.txt</a></li>"));
    }

    #[tokio::test]
    async fn test_render_directory_listing_with_directories() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join("dir1")).unwrap();
        fs::create_dir(temp_dir.path().join("dir2")).unwrap();

        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 应该包含目录链接（带 / 后缀）
        assert!(body.contains("<li><a href=\"./dir1/\">dir1/</a></li>"));
        assert!(body.contains("<li><a href=\"./dir2/\">dir2/</a></li>"));
    }

    #[tokio::test]
    async fn test_render_directory_listing_mixed_files_and_dirs() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::File::create(temp_dir.path().join("file.txt")).unwrap();

        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        assert!(body.contains("<li><a href=\"./subdir/\">subdir/</a></li>"));
        assert!(body.contains("<li><a href=\"./file.txt\">file.txt</a></li>"));
    }

    #[tokio::test]
    async fn test_render_directory_listing_nested_path() {
        let temp_dir = TempDir::new().unwrap();
        let nested = temp_dir.path().join("parent").join("child");
        fs::create_dir_all(&nested).unwrap();

        let result = render_directory_listing("parent/child", &nested).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 应该显示完整路径
        assert!(body.contains("<title>Index of /parent/child</title>"));
        assert!(body.contains("<h1>Index of /parent/child</h1>"));

        // 应该包含 ../ 链接（非根目录）
        assert!(body.contains("<li><a href=\"../\">../</a></li>"));
    }

    #[tokio::test]
    async fn test_render_directory_listing_sorting() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join("Zebra")).unwrap();
        fs::create_dir(temp_dir.path().join("apple")).unwrap();
        fs::File::create(temp_dir.path().join("Banana.txt")).unwrap();

        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 检查排序（不区分大小写）：apple, Banana, Zebra
        let apple_pos = body.find("apple").unwrap();
        let banana_pos = body.find("Banana").unwrap();
        let zebra_pos = body.find("Zebra").unwrap();

        assert!(apple_pos < banana_pos);
        assert!(banana_pos < zebra_pos);
    }

    #[tokio::test]
    async fn test_render_directory_listing_special_characters_in_filename() {
        let temp_dir = TempDir::new().unwrap();
        fs::File::create(temp_dir.path().join("file & test.txt")).unwrap();
        fs::File::create(temp_dir.path().join("<script>.txt")).unwrap();

        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 应该转义特殊字符
        assert!(body.contains("file &amp; test.txt"));
        assert!(body.contains("&lt;script&gt;.txt"));

        // 不应该包含未转义的特殊字符
        assert!(!body.contains("file & test.txt"));
        assert!(!body.contains("<script>"));
    }

    #[tokio::test]
    async fn test_render_directory_listing_nonexistent_directory() {
        let result =
            render_directory_listing("", Path::new("/nonexistent/path/that/does/not/exist")).await;

        assert!(result.is_err());
        match result {
            Err(SilentError::BusinessError { code, .. }) => {
                assert_eq!(code, StatusCode::NOT_FOUND);
            }
            _ => panic!("Expected BusinessError with NOT_FOUND status"),
        }
    }

    #[tokio::test]
    async fn test_render_directory_listing_file_instead_of_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not_a_dir.txt");
        fs::File::create(&file_path).unwrap();

        let result = render_directory_listing("", &file_path).await;

        assert!(result.is_err());
        match result {
            Err(SilentError::BusinessError { code, .. }) => {
                assert_eq!(code, StatusCode::NOT_FOUND);
            }
            _ => panic!("Expected BusinessError with NOT_FOUND status"),
        }
    }

    #[tokio::test]
    async fn test_render_directory_listing_response_content_type() {
        let temp_dir = TempDir::new().unwrap();
        let result = render_directory_listing("", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证 Content-Type 头（实际是 text/html，不包含 charset）
        assert_eq!(
            res.headers().get("content-type").unwrap().to_str().unwrap(),
            "text/html"
        );
    }

    #[tokio::test]
    async fn test_render_directory_listing_html_structure() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join("test_dir")).unwrap();

        let result = render_directory_listing("test/path", temp_dir.path()).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        let body = extract_body(res).await;

        // 验证完整的 HTML 结构
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("<html>"));
        assert!(body.contains("<head>"));
        assert!(body.contains("<meta charset=\"utf-8\">"));
        assert!(body.contains("<title>"));
        assert!(body.contains("</title>"));
        assert!(body.contains("<style>"));
        assert!(body.contains("</style>"));
        assert!(body.contains("</head>"));
        assert!(body.contains("<body>"));
        assert!(body.contains("<h1>"));
        assert!(body.contains("</h1>"));
        assert!(body.contains("<ul>"));
        assert!(body.contains("</ul>"));
        assert!(body.contains("</body>"));
        assert!(body.contains("</html>"));

        // 验证 CSS 样式
        assert!(body.contains("body{font-family:monospace;}"));
        assert!(body.contains("a{text-decoration:none;}"));
        assert!(body.contains("ul{list-style:none;padding-left:0;}"));
    }

    #[test]
    fn test_escape_html_all_special_chars_together() {
        let input = "&<>\"'";
        let expected = "&amp;&lt;&gt;&quot;&#39;";
        assert_eq!(escape_html(input), expected);
    }

    #[test]
    fn test_escape_html_repeated_special_chars() {
        assert_eq!(escape_html("&&&&"), "&amp;&amp;&amp;&amp;");
        assert_eq!(escape_html("<<<<"), "&lt;&lt;&lt;&lt;");
    }
}
