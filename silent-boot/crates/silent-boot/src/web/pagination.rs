//! 分页：查询参数里的 `page`、`page_size`，响应里的 [`PageResult`]。

use serde::{Deserialize, Serialize};
use silent::Request;
use silent::extractor::{FromRequest, Query};
use validator::{Validate, ValidationError, ValidationErrors};

use super::error::AppError;

/// `page_size` 的默认值。
pub const DEFAULT_PAGE_SIZE: u64 = 20;
/// `page_size` 的上限。
pub const MAX_PAGE_SIZE: u64 = 100;

/// 分页参数。页码从 1 起，`page_size` 默认 20，最大 100。
///
/// 查询 DTO 里不用带它：handler 的参数里加上 [`Page`] 提取器，它从同一个查询串里读 `page`、`page_size`：
///
/// ```ignore
/// async fn page(
///     (State(app), page, ValidatedQuery(query)): (State<AppState>, Page, ValidatedQuery<ArticleQueryDto>),
/// ) -> Result<Envelope<PageResult<ArticleDto>>> {
///     Ok(Envelope::ok(ArticleService::page(app.db.db(), &page.0, &query).await?))
/// }
/// ```
///
/// 查询串里的值都是字符串，而 `serde(flatten)` 会把数字当成字符串传给内层结构体，
/// 所以分页参数不放进 DTO，单独解析。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Validate)]
#[cfg_attr(
    feature = "openapi",
    derive(utoipa::IntoParams),
    into_params(parameter_in = Query)
)]
pub struct PageQuery {
    /// 页码，从 1 起。
    #[validate(range(min = 1))]
    #[cfg_attr(feature = "openapi", param(minimum = 1, example = 1))]
    pub page: Option<u64>,
    /// 每页条数，1 到 100。
    #[validate(range(min = 1, max = 100))]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100, example = 20))]
    pub page_size: Option<u64>,
}

impl PageQuery {
    /// 页码，从 1 起。
    pub fn page(&self) -> u64 {
        self.page.unwrap_or(1)
    }

    /// 每页条数。
    pub fn page_size(&self) -> u64 {
        self.page_size.unwrap_or(DEFAULT_PAGE_SIZE)
    }

    /// 这一页跳过的行数。
    pub fn offset(&self) -> u64 {
        (self.page() - 1) * self.page_size()
    }
}

/// 从查询参数里取 [`PageQuery`] 的提取器。`page`、`page_size` 不是数字或者超出范围时是 422。
pub struct Page(pub PageQuery);

#[async_trait::async_trait]
impl FromRequest for Page {
    type Rejection = AppError;

    async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            page: Option<String>,
            #[serde(default)]
            page_size: Option<String>,
        }
        let Query(raw) = Query::<Raw>::from_request(req)
            .await
            .map_err(|e| AppError::bad_request(e.message()))?;
        let mut errors = ValidationErrors::new();
        let page = parse_number("page", raw.page, &mut errors);
        let page_size = parse_number("page_size", raw.page_size, &mut errors);
        let query = PageQuery { page, page_size };
        if errors.errors().is_empty() {
            query.validate()?;
        } else {
            return Err(AppError::Validation(errors));
        }
        Ok(Self(query))
    }
}

/// 空串或者没传是 `None`；不是数字就记一条 `parse` 错误。
fn parse_number(
    field: &'static str,
    raw: Option<String>,
    errors: &mut ValidationErrors,
) -> Option<u64> {
    let raw = raw?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match raw.parse::<u64>() {
        Ok(value) => Some(value),
        Err(_) => {
            errors.add(field, ValidationError::new("parse"));
            None
        }
    }
}

/// 一页数据。序列化成 `{"page":1,"pageSize":20,"total":100,"items":[..]}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PageResult<T> {
    /// 页码，从 1 起。
    pub page: u64,
    /// 每页条数。
    pub page_size: u64,
    /// 满足条件的总行数。
    pub total: u64,
    /// 这一页的行。
    pub items: Vec<T>,
}

impl<T> PageResult<T> {
    /// 用查询参数里的页码和条数组装。
    pub fn new(query: &PageQuery, total: u64, items: Vec<T>) -> Self {
        Self {
            page: query.page(),
            page_size: query.page_size(),
            total,
            items,
        }
    }
}

/// 执行一次分页查询，把每行映射成 `T`。
///
/// ```ignore
/// let select = article::Entity::find().filter(article::Column::DeletedAt.is_null());
/// let page = paginate(db, select, &query.page, ArticleDto::from).await?;
/// ```
pub async fn paginate<'db, DB, S, T, F>(
    db: &'db DB,
    select: S,
    query: &PageQuery,
    mut map: F,
) -> Result<PageResult<T>, sea_orm::DbErr>
where
    DB: sea_orm::ConnectionTrait + 'db,
    S: sea_orm::PaginatorTrait<'db, DB> + 'db,
    S::Selector: sea_orm::SelectorTrait + Send + Sync + 'db,
    F: FnMut(<S::Selector as sea_orm::SelectorTrait>::Item) -> T,
{
    let paginator = select.paginate(db, query.page_size());
    let total = paginator.num_items().await?;
    let items = paginator
        .fetch_page(query.page() - 1)
        .await?
        .into_iter()
        .map(&mut map)
        .collect();
    Ok(PageResult::new(query, total, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_bounds() {
        let query = PageQuery::default();
        assert_eq!(query.page(), 1);
        assert_eq!(query.page_size(), DEFAULT_PAGE_SIZE);
        assert_eq!(query.offset(), 0);

        let query = PageQuery {
            page: Some(3),
            page_size: Some(10),
        };
        assert_eq!(query.offset(), 20);

        let query = PageQuery {
            page: Some(0),
            page_size: Some(101),
        };
        assert!(query.validate().is_err());
    }

    fn route() -> silent::prelude::Route {
        use silent::prelude::*;
        Route::new("").get(|page: Page| async move { Ok(page.0.offset().to_string()) })
    }

    #[tokio::test]
    async fn page_extractor_defaults_parses_and_rejects() {
        use silent::StatusCode;
        use silent::testing::TestClient;
        let route = route();
        let res = TestClient::get("/").send(&route).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.text().await, "0");

        let res = TestClient::get("/?page=3&page_size=10").send(&route).await;
        assert_eq!(res.text().await, "20");

        // 数字不对：提取器的拒绝响应直接是包络。
        let res = TestClient::get("/?page=abc").send(&route).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json: serde_json::Value = res.json().await;
        assert_eq!(json["code"], 42_200);
        assert_eq!(json["data"]["page"][0]["code"], "parse");

        let res = TestClient::get("/?page_size=101").send(&route).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json: serde_json::Value = res.json().await;
        assert_eq!(json["data"]["page_size"][0]["code"], "range");
    }

    #[test]
    fn result_is_camel_case() {
        let query = PageQuery::default();
        let page = PageResult::new(&query, 0, Vec::<u8>::new());
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "page": 1, "pageSize": 20, "total": 0, "items": [] })
        );
    }
}
