//! 两个中间件：[`TraceId`] 给每个请求发追踪号，[`ErrorHandler`] 把失败统一成包络。

use std::panic::AssertUnwindSafe;

use futures::{FutureExt, StreamExt};
use silent::header::CONTENT_TYPE;
use silent::{
    Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError, StatusCode,
};
use tracing::Instrument;

use super::error::{AppError, Envelope};

/// 追踪号所在的请求头和响应头。
pub const TRACE_HEADER: &str = "x-request-id";

/// 超过这个长度的响应体不检查：大文件、导出、流式响应原样返回。
const MAX_INSPECTED_BODY: usize = 1024 * 1024;

/// 给每个请求一个追踪号。
///
/// 请求头里带了合法的 `x-request-id`（最长 64 个字符，只含字母、数字、`-`、`_`、`.`）就沿用，
/// 否则新生成一个。追踪号会写回请求头、响应头，并放进 `request` 这个 tracing span，
/// 所以这次请求里打的日志都带着 `trace_id`。
#[derive(Debug, Clone, Copy, Default)]
pub struct TraceId;

#[async_trait::async_trait]
impl MiddleWareHandler for TraceId {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        let id = req
            .headers()
            .get(TRACE_HEADER)
            .and_then(|value| value.to_str().ok())
            .filter(|value| is_valid_trace_id(value))
            .map(str::to_owned)
            .unwrap_or_else(scru128::new_string);
        if let Ok(value) = id.parse() {
            req.headers_mut().insert(TRACE_HEADER, value);
        }
        let span = tracing::info_span!("request", trace_id = %id);
        let mut res = match next.call(req).instrument(span).await {
            Ok(res) => res,
            // 走到这里的错误来自 ErrorHandler 外面的中间件（例如超时），按 Silent 的默认规则转成响应。
            Err(e) => Response::from(e),
        };
        if let Ok(value) = id.parse() {
            res.headers_mut().insert(TRACE_HEADER, value);
        }
        Ok(res)
    }
}

/// 把失败统一成 [`Envelope`]，并给成功的包络补上追踪号。
///
/// 覆盖三类失败：
///
/// - handler 返回的 `Err`：由 [`AppError`] 生成的还原出业务码；其余按状态码生成，5xx 不暴露细节；
/// - 提取器的拒绝响应：Silent 的内置提取器失败时直接返回纯文本，不经过 `ExceptionHandler`（M0 实测），
///   这里把 4xx、5xx 的非包络响应体改写成包络；
/// - handler 里的 panic：记日志，返回 500 的包络，连接不会因此断开。
///
/// 成功的 JSON 响应如果是包络的形状（有 `code`、`message`、`data`）而缺 `traceId`，会补上追踪号。
#[derive(Debug, Clone, Copy, Default)]
pub struct ErrorHandler;

#[async_trait::async_trait]
impl MiddleWareHandler for ErrorHandler {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let trace_id = current_trace_id(&req);
        let outcome = AssertUnwindSafe(next.call(req)).catch_unwind().await;
        let mut res = match outcome {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => render(&e, &trace_id),
            Err(panic) => {
                let detail = panic_message(&panic);
                tracing::error!(panic = %detail, "handler panicked");
                error_response(
                    AppError::internal(anyhow::anyhow!("handler panicked: {detail}"))
                        .to_envelope()
                        .with_trace_id(&trace_id),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            }
        };
        res = normalize(res, &trace_id).await;
        Ok(res)
    }
}

/// handler 返回的 `Err` 变成失败响应。
fn render(err: &SilentError, trace_id: &str) -> Response {
    let status = err.status();
    let mut envelope = decode_envelope(&err.message()).unwrap_or_else(|| {
        let message = if status.is_server_error() {
            "服务器内部错误".to_owned()
        } else {
            err.message()
        };
        Envelope::new(
            u32::from(status.as_u16()) * 100,
            message,
            serde_json::Value::Null,
        )
    });
    // 包络里的业务码和 HTTP 状态码对不上时，以状态码为准。
    if business_status(envelope.code) != Some(status) {
        envelope.code = u32::from(status.as_u16()) * 100;
    }
    if status.is_server_error() {
        tracing::error!(status = status.as_u16(), error = %err, "request failed");
    }
    error_response(envelope.with_trace_id(trace_id), status)
}

/// 成功的包络补上追踪号；提取器产生的纯文本错误响应改成包络。其余响应原样返回。
async fn normalize(mut res: Response, trace_id: &str) -> Response {
    if res.content_length().lower() > MAX_INSPECTED_BODY as u64 {
        return res;
    }
    let status = res.status();
    let Some(body) = take_body(&mut res).await else {
        let fallback = Envelope::new(50_000, "服务器内部错误", serde_json::Value::Null);
        return error_response(
            fallback.with_trace_id(trace_id),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    };

    if status.is_success() {
        if is_json(&res)
            && let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body)
            && envelope_shaped(&value)
            && trace_id_missing(&value)
            && let Some(object) = value.as_object_mut()
        {
            object.insert(
                "traceId".to_owned(),
                serde_json::Value::String(trace_id.to_owned()),
            );
            res.set_body(silent::prelude::full(
                serde_json::to_vec(&value).unwrap_or_default(),
            ));
            return res;
        }
        res.set_body(silent::prelude::full(body));
        return res;
    }

    // 4xx、5xx：不是包络的响应体改成包络。
    let parsed = serde_json::from_slice::<serde_json::Value>(&body).ok();
    let envelope = match parsed.as_ref().filter(|value| envelope_shaped(value)) {
        Some(value) => Envelope::new(
            value["code"].as_u64().unwrap_or_default() as u32,
            value["message"].as_str().unwrap_or_default().to_owned(),
            value
                .get("data")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        ),
        None => {
            let text = String::from_utf8_lossy(&body);
            let text = text.trim();
            let message = if status.is_server_error() {
                "服务器内部错误".to_owned()
            } else if text.is_empty() {
                status
                    .canonical_reason()
                    .unwrap_or("request failed")
                    .to_owned()
            } else {
                truncate(text, 200)
            };
            Envelope::new(
                u32::from(status.as_u16()) * 100,
                message,
                serde_json::Value::Null,
            )
        }
    };
    error_response(envelope.with_trace_id(trace_id), status)
}

fn error_response(envelope: Envelope<serde_json::Value>, status: StatusCode) -> Response {
    Response::json(&envelope).with_status(status)
}

fn business_status(code: u32) -> Option<StatusCode> {
    StatusCode::from_u16(u16::try_from(code / 100).ok()?).ok()
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

fn current_trace_id(req: &Request) -> String {
    req.headers()
        .get(TRACE_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

/// `SilentError` 的消息里是不是 [`AppError`] 留下的包络。
fn decode_envelope(message: &str) -> Option<Envelope<serde_json::Value>> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    if !envelope_shaped(&value) {
        return None;
    }
    Some(Envelope::new(
        value["code"].as_u64()? as u32,
        value["message"].as_str()?.to_owned(),
        value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    ))
}

/// 有整数的 `code`、字符串的 `message` 和 `data` 这三个键。
fn envelope_shaped(value: &serde_json::Value) -> bool {
    value.is_object()
        && value["code"].is_u64()
        && value["message"].is_string()
        && value.get("data").is_some()
}

fn trace_id_missing(value: &serde_json::Value) -> bool {
    match value.get("traceId").and_then(|v| v.as_str()) {
        Some(s) => s.is_empty(),
        None => true,
    }
}

fn is_json(res: &Response) -> bool {
    res.headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let mime = value.split(';').next().unwrap_or("").trim();
            mime.eq_ignore_ascii_case("application/json") || mime.ends_with("+json")
        })
        .unwrap_or(false)
}

/// 读完响应体。读不出来时返回 `None`。
async fn take_body(res: &mut Response) -> Option<Vec<u8>> {
    let body = res.take_body();
    let mut body = std::pin::pin!(body);
    let mut buf = Vec::new();
    while let Some(chunk) = body.next().await {
        buf.extend_from_slice(&chunk.ok()?);
    }
    Some(buf)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

/// 追踪号的合法形态：1 到 64 个字符，只含字母、数字、`-`、`_`、`.`。
fn is_valid_trace_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}
