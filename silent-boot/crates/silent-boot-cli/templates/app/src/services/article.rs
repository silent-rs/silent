//! 文章的业务逻辑：查询、新建、修改、软删除。
//!
//! 审计字段（`created_at`、`updated_at`、`created_by`、`updated_by`）在这里写入：
//! SeaORM 不会自动填充，实体的文件也不手改。

use entity::article;
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use silent_boot::prelude::*;

use crate::dto::article::{ArticleDto, ArticleQueryDto, CreateArticleDto, UpdateArticleDto};

pub struct ArticleService;

/// 当前用户的 ID。接上认证之后换成从请求里取到的值。
const SYSTEM_USER: &str = "system";

impl ArticleService {
    /// 分页查询，只返回没删除的文章。
    pub async fn page(
        db: &sea_orm::DatabaseConnection,
        page: &PageQuery,
        query: &ArticleQueryDto,
    ) -> AppResult<PageResult<ArticleDto>> {
        let mut select = article::Entity::find().filter(article::Column::DeletedAt.is_null());
        if let Some(keyword) = query.keyword.as_deref().filter(|k| !k.is_empty()) {
            select = select.filter(article::Column::Title.contains(keyword));
        }
        let select = select.order_by_desc(article::Column::CreatedAt);
        Ok(paginate(db, select, page, ArticleDto::from).await?)
    }

    /// 按 ID 取一篇，删除的不算。
    pub async fn get(db: &sea_orm::DatabaseConnection, id: &str) -> AppResult<ArticleDto> {
        existing(db, id).await.map(ArticleDto::from)
    }

    pub async fn create(
        db: &sea_orm::DatabaseConnection,
        input: &CreateArticleDto,
    ) -> AppResult<ArticleDto> {
        let now = chrono::Local::now().naive_local();
        let model = article::ActiveModel {
            id: Set(scru128::new_string()),
            title: Set(input.title.clone()),
            content: Set(input.content.clone()),
            status: Set(0),
            created_by: Set(Some(SYSTEM_USER.to_owned())),
            updated_by: Set(Some(SYSTEM_USER.to_owned())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok(model.into())
    }

    /// 没传的字段不动。
    pub async fn update(
        db: &sea_orm::DatabaseConnection,
        id: &str,
        input: &UpdateArticleDto,
    ) -> AppResult<ArticleDto> {
        let mut model: article::ActiveModel = existing(db, id).await?.into();
        if let Some(title) = &input.title {
            model.title = Set(title.clone());
        }
        if let Some(content) = &input.content {
            model.content = Set(Some(content.clone()));
        }
        if let Some(status) = input.status {
            model.status = Set(status);
        }
        model.updated_by = Set(Some(SYSTEM_USER.to_owned()));
        model.updated_at = Set(chrono::Local::now().naive_local());
        Ok(model.update(db).await?.into())
    }

    /// 软删除：写 `deleted_at`，不删行。
    pub async fn remove(db: &sea_orm::DatabaseConnection, id: &str) -> AppResult<()> {
        let result = article::Entity::update_many()
            .col_expr(
                article::Column::DeletedAt,
                Expr::value(chrono::Local::now().naive_local()),
            )
            .col_expr(
                article::Column::UpdatedBy,
                Expr::value(Some(SYSTEM_USER.to_owned())),
            )
            .filter(article::Column::Id.eq(id))
            .filter(article::Column::DeletedAt.is_null())
            .exec(db)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::not_found("文章"));
        }
        Ok(())
    }
}

/// 取一篇没删除的文章，取不到就是 404。
async fn existing(db: &sea_orm::DatabaseConnection, id: &str) -> AppResult<article::Model> {
    article::Entity::find_by_id(id)
        .filter(article::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| AppError::not_found("文章"))
}
