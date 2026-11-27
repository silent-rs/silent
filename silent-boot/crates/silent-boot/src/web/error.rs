//! 统一响应与错误，见 `docs/design.md` §5。
//!
//! 成功与失败同构，都是 [`Envelope`]：
//!
//! ```json
//! { "code": 200, "message": "success", "data": { "page": 1, "pageSize": 20, "total": 100, "items": [] }, "traceId": "…" }
//! { "code": 40301, "message": "无权限：article:delete", "data": null, "traceId": "…" }
//! ```
//!
//! - HTTP 状态码始终有意义，`code` 是细分业务码，规则是 `HTTP 状态码 * 100 + 序号`。
//! - service 层用 [`AppError`] 表达失败；handler 写成 `Result<Envelope<T>, SilentError>`，对 service 的结果用 `?`。
//! - 失败响应由 [`super::handler::ErrorHandler`] 统一生成；Silent 内置提取器的拒绝响应也会被改写成包络。

use serde::Serialize;
use silent::{Response, SilentError, StatusCode};

/// service 层的返回类型：失败是 [`AppError`]。SeaORM 的 `DbErr` 用 `?` 就能转过来。
///
/// handler 的返回类型用 Silent 的 `Result<T>`（也就是 `Result<T, SilentError>`）：成功值写成
/// [`Envelope<T>`]（`Ok(Envelope::ok(dto))`），需要 201 时写成 `Ok(Created(Envelope::created(dto)))`。
/// handler 里对 service 的结果直接用 `?`，[`AppError`] 会自动转成 `SilentError`。
pub type AppResult<T> = std::result::Result<T, AppError>;

/// 响应包络。
///
/// 启用 `openapi` feature 时它也实现 `utoipa::ToSchema`，文档里写 `body = Envelope<ArticleDto>`。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Envelope<T = serde_json::Value> {
    /// 业务码。成功时等于 HTTP 状态码。
    pub code: u32,
    /// 给人看的说明。
    pub message: String,
    /// 数据。失败时是 `null` 或者字段错误。
    pub data: T,
    /// 这次请求的追踪号，和响应头 `x-request-id`、日志里的 `trace_id` 是同一个值。
    #[serde(rename = "traceId")]
    pub trace_id: String,
}

impl<T> Envelope<T> {
    /// 自己指定业务码和说明。
    pub fn new(code: u32, message: impl Into<String>, data: T) -> Self {
        Self {
            code,
            message: message.into(),
            data,
            trace_id: String::new(),
        }
    }

    /// 成功：`code = 200`，说明是 `success`。
    pub fn ok(data: T) -> Self {
        Self::new(200, "success", data)
    }

    /// 成功：`code = 201`，用于新建资源。
    pub fn created(data: T) -> Self {
        Self::new(201, "created", data)
    }

    /// 补上追踪号。已有追踪号时不动。
    pub fn with_trace_id(mut self, trace_id: &str) -> Self {
        if self.trace_id.is_empty() && !trace_id.is_empty() {
            self.trace_id = trace_id.to_owned();
        }
        self
    }
}

/// `201 Created`：handler 返回 `Ok(Created(Envelope::created(dto)))`，返回类型写 `Result<Created<ArticleDto>>`。
///
/// 没有实现 `Serialize` 是有意的：Silent 给所有能序列化的类型都实现了 `Into<Response>`，
/// 这里要自己定状态码。
#[derive(Debug, Clone, PartialEq)]
pub struct Created<T>(pub Envelope<T>);

impl<T: Serialize> From<Created<T>> for Response {
    fn from(value: Created<T>) -> Self {
        Response::json(&value.0).with_status(StatusCode::CREATED)
    }
}

/// 业务错误。
///
/// 业务码由 HTTP 状态码推出来：`状态码 * 100`，再加序号。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 401 / 40100：没有登录或者令牌失效。
    #[error("未登录")]
    Unauthorized,
    /// 403 / 40301：没有权限，带权限码。
    #[error("无权限：{0}")]
    Forbidden(String),
    /// 404 / 40400：资源不存在。
    #[error("{0}不存在")]
    NotFound(String),
    /// 400 / 40000：请求本身不成立，例如 JSON 不是合法的、缺少必需的参数。
    #[error("请求错误：{0}")]
    BadRequest(String),
    /// 422 / 42200：参数校验失败，带每个字段的错误。
    #[error("参数校验失败")]
    Validation(#[from] validator::ValidationErrors),
    /// 409 / 40900：和现有数据冲突，例如唯一键重复。
    #[error("数据冲突：{0}")]
    Conflict(String),
    /// 429 / 42900：请求太频繁。
    #[error("请求过于频繁，请稍后再试")]
    TooManyRequests,
    /// 自定义业务码。HTTP 状态码取业务码的前三位；前三位不是 4xx、5xx 时按 400 返回。
    #[error("{1}")]
    Business(u32, String),
    /// 500 / 50000：没预料到的错误。详情只写日志，对外只说"服务器内部错误"。
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    /// 资源不存在：`AppError::not_found("文章")` 得到 `文章不存在`。
    pub fn not_found(what: impl Into<String>) -> Self {
        Self::NotFound(what.into())
    }

    /// 请求本身不成立。
    pub fn bad_request(reason: impl Into<String>) -> Self {
        Self::BadRequest(reason.into())
    }

    /// 和现有数据冲突。
    pub fn conflict(reason: impl Into<String>) -> Self {
        Self::Conflict(reason.into())
    }

    /// 没预料到的错误。
    pub fn internal(err: impl Into<anyhow::Error>) -> Self {
        Self::Internal(err.into())
    }

    /// 对外的 HTTP 状态码。
    pub fn http_status(&self) -> StatusCode {
        let code = self.business_code();
        business_status(code).unwrap_or(StatusCode::BAD_REQUEST)
    }

    /// 对外的业务码。
    pub fn business_code(&self) -> u32 {
        match self {
            Self::Unauthorized => 40_100,
            Self::Forbidden(_) => 40_301,
            Self::NotFound(_) => 40_400,
            Self::BadRequest(_) => 40_000,
            Self::Validation(_) => 42_200,
            Self::Conflict(_) => 40_900,
            Self::TooManyRequests => 42_900,
            Self::Business(code, _) => *code,
            Self::Internal(_) => 50_000,
        }
    }

    /// 对外的说明。内部错误不暴露细节。
    pub fn public_message(&self) -> String {
        match self {
            Self::Internal(_) => "服务器内部错误".to_owned(),
            other => other.to_string(),
        }
    }

    /// 5xx 的错误才值得写日志。
    pub fn is_internal(&self) -> bool {
        matches!(self, Self::Internal(_))
    }

    /// 变成失败响应的包络。`data` 里是字段错误，没有就是 `null`。
    pub fn to_envelope(&self) -> Envelope<serde_json::Value> {
        let data = match self {
            Self::Validation(errors) => field_errors(errors),
            _ => serde_json::Value::Null,
        };
        Envelope::new(self.business_code(), self.public_message(), data)
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        match err {
            sea_orm::DbErr::RecordNotFound(_) => Self::NotFound("记录".to_owned()),
            other => Self::Internal(other.into()),
        }
    }
}

/// 业务码对应回 HTTP 状态码：取前三位，只有 4xx、5xx 算数。
pub fn business_status(code: u32) -> Option<StatusCode> {
    let status = StatusCode::from_u16(u16::try_from(code / 100).ok()?).ok()?;
    (status.is_client_error() || status.is_server_error()).then_some(status)
}

/// 字段错误变成 `{"字段": [{"code": "规则", "message": "说明", "params": {..}}]}`。
///
/// 嵌套的结构按字段名继续展开，列表按下标展开。
fn field_errors(errors: &validator::ValidationErrors) -> serde_json::Value {
    use validator::ValidationErrorsKind;
    let mut out = serde_json::Map::new();
    for (field, kind) in errors.errors() {
        let value = match kind {
            ValidationErrorsKind::Field(errors) => serde_json::Value::Array(
                errors
                    .iter()
                    .map(|e| {
                        let params = serde_json::to_value(&e.params).unwrap_or(serde_json::Value::Null);
                        serde_json::json!({
                            "code": e.code.clone(),
                            "message": e.message.clone().unwrap_or_else(|| describe(&e.code, &params)),
                            "params": params,
                        })
                    })
                    .collect(),
            ),
            ValidationErrorsKind::Struct(errors) => field_errors(errors),
            ValidationErrorsKind::List(errors) => {
                let mut list = serde_json::Map::new();
                for (index, errors) in errors {
                    list.insert(index.to_string(), field_errors(errors));
                }
                serde_json::Value::Object(list)
            }
        };
        out.insert(field.to_string(), value);
    }
    serde_json::Value::Object(out)
}

/// 按规则生成一句中文说明。规则自己带了 `message` 的不会走到这里。
fn describe(code: &str, params: &serde_json::Value) -> std::borrow::Cow<'static, str> {
    let number = |key: &str| params[key].as_f64();
    let text = |key: &str| params[key].as_str().unwrap_or_default().to_owned();
    let bounds = |unit: &str| match (number("min"), number("max")) {
        (Some(min), Some(max)) => format!("{unit}必须在 {min} 到 {max} 之间"),
        (Some(min), None) => format!("{unit}不能小于 {min}"),
        (None, Some(max)) => format!("{unit}不能大于 {max}"),
        _ => format!("{unit}不合法"),
    };
    let message = match code {
        "length" => match number("equal") {
            Some(equal) => format!("长度必须是 {equal}"),
            None => bounds("长度"),
        },
        "range" => bounds("取值"),
        "required" | "required_nested" => "不能为空".to_owned(),
        "email" => "不是有效的邮箱".to_owned(),
        "url" => "不是有效的地址".to_owned(),
        "regex" => "格式不正确".to_owned(),
        "must_match" => format!("必须和 {} 一致", text("matcher")),
        "contains" => format!("必须包含 {}", text("pattern")),
        "does_not_contain" => format!("不能包含 {}", text("pattern")),
        "ip" => "不是有效的 IP 地址".to_owned(),
        "non_control_character" => "不能包含控制字符".to_owned(),
        "must_be_true" => "必须为真".to_owned(),
        "must_be_false" => "必须为假".to_owned(),
        _ => "不满足要求".to_owned(),
    };
    message.into()
}

/// handler 里用 `?` 把 [`AppError`] 交给 Silent。
///
/// 业务码和字段错误以 JSON 载荷写进 `SilentError`，由 [`super::handler::ErrorHandler`] 还原成包络。
impl From<AppError> for SilentError {
    fn from(err: AppError) -> Self {
        SilentError::business_error_obj(err.http_status(), err.to_envelope())
    }
}

/// 提取器的拒绝响应直接是包络。
impl From<AppError> for Response {
    fn from(err: AppError) -> Self {
        Response::json(&err.to_envelope()).with_status(err.http_status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_codes_follow_the_http_status() {
        let cases: [(AppError, StatusCode, u32); 8] = [
            (AppError::Unauthorized, StatusCode::UNAUTHORIZED, 40_100),
            (
                AppError::Forbidden("article:delete".to_owned()),
                StatusCode::FORBIDDEN,
                40_301,
            ),
            (AppError::not_found("文章"), StatusCode::NOT_FOUND, 40_400),
            (
                AppError::bad_request("缺标题"),
                StatusCode::BAD_REQUEST,
                40_000,
            ),
            (AppError::conflict("重名"), StatusCode::CONFLICT, 40_900),
            (
                AppError::TooManyRequests,
                StatusCode::TOO_MANY_REQUESTS,
                42_900,
            ),
            (
                AppError::Business(40_401, "已归档".to_owned()),
                StatusCode::NOT_FOUND,
                40_401,
            ),
            (
                AppError::internal(anyhow::anyhow!("boom")),
                StatusCode::INTERNAL_SERVER_ERROR,
                50_000,
            ),
        ];
        for (err, status, code) in cases {
            assert_eq!(err.http_status(), status, "{err:?}");
            assert_eq!(err.business_code(), code, "{err:?}");
        }
    }

    #[test]
    fn custom_codes_outside_client_and_server_ranges_are_bad_requests() {
        assert_eq!(
            AppError::Business(20_001, "ok".to_owned()).http_status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(business_status(999), None);
        assert_eq!(
            business_status(50_300),
            Some(StatusCode::SERVICE_UNAVAILABLE)
        );
    }

    #[test]
    fn internal_errors_do_not_leak_details() {
        let err = AppError::internal(anyhow::anyhow!("secret path /etc/passwd"));
        assert!(err.is_internal());
        assert_eq!(err.public_message(), "服务器内部错误");
        assert!(!err.to_envelope().message.contains("passwd"));

        let silent: SilentError = err.into();
        let body: serde_json::Value = serde_json::from_str(&silent.message()).unwrap();
        assert_eq!(body["code"], 50_000);
        assert_eq!(body["message"], "服务器内部错误");
        assert_eq!(silent.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn validation_errors_carry_field_details() {
        use validator::Validate;

        #[derive(validator::Validate)]
        struct Input {
            #[validate(length(min = 1))]
            title: String,
        }
        let err = AppError::Validation(
            Input {
                title: String::new(),
            }
            .validate()
            .unwrap_err(),
        );
        let envelope = err.to_envelope();
        assert_eq!(envelope.code, 42_200);
        assert_eq!(envelope.data["title"][0]["code"], "length");
        assert_eq!(envelope.data["title"][0]["message"], "长度不能小于 1");
        assert_eq!(envelope.data["title"][0]["params"]["min"], 1);
    }

    #[test]
    fn envelopes_render_as_designed() {
        let json = serde_json::to_value(Envelope::ok(serde_json::json!({ "page": 1 }))).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "code": 200, "message": "success", "data": { "page": 1 }, "traceId": "" })
        );
        let created: Response = Created(Envelope::created(1)).into();
        assert_eq!(created.status(), StatusCode::CREATED);

        let envelope = Envelope::<u8>::new(1, "m", 2).with_trace_id("t1");
        assert_eq!(envelope.trace_id, "t1");
        let kept = envelope.clone().with_trace_id("t2");
        assert_eq!(kept.trace_id, "t1");
    }

    #[test]
    fn database_errors_map_to_internal_or_not_found() {
        let err: AppError = sea_orm::DbErr::RecordNotFound("x".to_owned()).into();
        assert_eq!(err.http_status(), StatusCode::NOT_FOUND);
        let err: AppError = sea_orm::DbErr::Custom("x".to_owned()).into();
        assert_eq!(err.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
