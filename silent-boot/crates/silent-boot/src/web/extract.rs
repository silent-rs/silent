//! 带校验的提取器：解析失败是 400，校验失败是 422，两种都直接是包络。

use serde::de::DeserializeOwned;
use silent::Request;
use silent::extractor::{FromRequest, Json, Query};
use validator::Validate;

use super::error::AppError;

/// `Json<T>` 加校验。请求体不是合法 JSON 时是 400，不满足 `#[validate(..)]` 规则时是 422。
///
/// ```ignore
/// async fn create(
///     (State(app), ValidatedJson(input)): (State<AppState>, ValidatedJson<CreateArticleDto>),
/// ) -> AppResult<Envelope<ArticleDto>> {
///     Ok(Envelope::ok(ArticleService.create(&app.db, &input).await?))
/// }
/// ```
pub struct ValidatedJson<T>(pub T);

#[async_trait::async_trait]
impl<T> FromRequest for ValidatedJson<T>
where
    T: DeserializeOwned + Validate + Send + 'static,
{
    type Rejection = AppError;

    async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req)
            .await
            .map_err(|e| AppError::bad_request(e.message()))?;
        value.validate()?;
        Ok(Self(value))
    }
}

/// `Query<T>` 加校验。解析失败是 400，不满足 `#[validate(..)]` 规则时是 422。
pub struct ValidatedQuery<T>(pub T);

#[async_trait::async_trait]
impl<T> FromRequest for ValidatedQuery<T>
where
    T: DeserializeOwned + Validate + Send + 'static,
{
    type Rejection = AppError;

    async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request(req)
            .await
            .map_err(|e| AppError::bad_request(e.message()))?;
        value.validate()?;
        Ok(Self(value))
    }
}
