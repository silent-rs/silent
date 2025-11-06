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

    entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

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
