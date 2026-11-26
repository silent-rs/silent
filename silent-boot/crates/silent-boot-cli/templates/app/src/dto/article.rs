//! 文章的入参和出参。

use entity::article;
use silent_boot::prelude::*;

/// 新建文章。
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateArticleDto {
    /// 标题，1 到 200 个字符。
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    /// 正文，可以为空。
    pub content: Option<String>,
}

/// 修改文章。没传的字段不动。
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateArticleDto {
    #[validate(length(min = 1, max = 200))]
    pub title: Option<String>,
    pub content: Option<String>,
    /// 0 草稿，1 已发布。
    #[validate(range(min = 0, max = 1))]
    pub status: Option<i8>,
}

/// 列表的查询条件。分页参数由 `Page` 提取器单独取，这里只有业务上的过滤条件。
///
/// `ValidatedQuery` 要求类型实现 `Validate`；没有规则也要派生一个空的。
#[derive(Debug, Deserialize, Validate, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ArticleQueryDto {
    /// 标题包含的关键字。
    #[validate(length(max = 100))]
    pub keyword: Option<String>,
}

/// 文章的出参。
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArticleDto {
    pub id: String,
    pub title: String,
    pub content: Option<String>,
    /// 0 草稿，1 已发布。
    pub status: i8,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl From<article::Model> for ArticleDto {
    fn from(model: article::Model) -> Self {
        Self {
            id: model.id,
            title: model.title,
            content: model.content,
            status: model.status,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}
