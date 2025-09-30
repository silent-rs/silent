use http::header::{ACCEPT_ENCODING, CONTENT_ENCODING, VARY};

use crate::{Request, Response};

use super::StaticOptions;

pub(super) enum Compression {
    Brotli,
    Gzip,
}

pub(super) fn negotiate(
    options: &StaticOptions,
    req: &Request,
    content_type: Option<&mime::Mime>,
) -> Option<Compression> {
    if !options.enable_compression {
        return None;
    }
    let content_type = content_type?;
    if !is_mime_compressible(content_type) {
        return None;
    }
    let header = req.headers().get(ACCEPT_ENCODING)?;
    let value = header.to_str().ok()?;
    parse_accept_encoding(value)
}

pub(super) fn apply_headers(res: &mut Response, compression: &Compression) {
    let (encoding, vary) = match compression {
        Compression::Brotli => ("br", "Accept-Encoding"),
        Compression::Gzip => ("gzip", "Accept-Encoding"),
    };
    res.headers_mut()
        .insert(CONTENT_ENCODING, encoding.parse().unwrap());
    res.headers_mut().insert(VARY, vary.parse().unwrap());
}

fn parse_accept_encoding(header: &str) -> Option<Compression> {
    let mut brotli = None;
    let mut gzip = None;
    for (index, item) in header.split(',').enumerate() {
        let item = item.trim();
        let mut parts = item.split(';');
        let encoding = parts.next()?.trim();
        let mut quality = 1.0_f32;
        for param in parts {
            let mut kv = param.splitn(2, '=');
            if kv.next().map(|p| p.trim()) == Some("q")
                && let Some(v) = kv.next()
                && let Ok(parsed) = v.trim().parse::<f32>()
            {
                quality = parsed;
            }
        }
        if quality == 0.0 {
            continue;
        }
        match encoding {
            "br" => {
                brotli.get_or_insert(index);
            }
            "gzip" | "x-gzip" => {
                gzip.get_or_insert(index);
            }
            "*" => {
                gzip.get_or_insert(index + 1000);
            }
            _ => {}
        }
    }
    if brotli.is_some() {
        Some(Compression::Brotli)
    } else if gzip.is_some() {
        Some(Compression::Gzip)
    } else {
        None
    }
}

fn is_mime_compressible(mime: &mime::Mime) -> bool {
    matches!(
        (mime.type_(), mime.subtype().as_str()),
        (mime::TEXT, _)
            | (mime::APPLICATION, "json")
            | (mime::APPLICATION, "xml")
            | (mime::APPLICATION, "javascript")
            | (mime::APPLICATION, "ecmascript")
            | (mime::APPLICATION, "x-javascript")
            | (mime::APPLICATION, "xhtml+xml")
            | (mime::APPLICATION, "rss+xml")
            | (mime::APPLICATION, "svg+xml")
            | (mime::IMAGE, "svg+xml")
    )
}
