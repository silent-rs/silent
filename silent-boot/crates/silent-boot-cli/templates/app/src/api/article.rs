//! 文章接口：从请求里取参数，调用 service，返回包络。这里不写业务逻辑，也不直接操作实体。

use silent_boot::prelude::*;

use crate::dto::article::{ArticleDto, ArticleQueryDto, CreateArticleDto, UpdateArticleDto};
use crate::services::article::ArticleService;

pub fn route() -> Route {
    Route::new("articles")
        .get(page)
        .post(create)
        .append(Route::new("<id>").get(detail).put(update).delete(remove))
}

#[utoipa::path(
    get,
    path = "/api/articles",
    params(ArticleQueryDto, PageQuery),
    responses((status = 200, body = Envelope<PageResult<ArticleDto>>))
)]
async fn page(
    (State(app), page, ValidatedQuery(query)): (
        State<AppState>,
        Page,
        ValidatedQuery<ArticleQueryDto>,
    ),
) -> Result<Envelope<PageResult<ArticleDto>>> {
    Ok(Envelope::ok(
        ArticleService::page(app.db.db(), &page.0, &query).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/articles",
    request_body = CreateArticleDto,
    responses(
        (status = 201, body = Envelope<ArticleDto>),
        (status = 422, description = "参数校验失败，data 里是字段错误")
    )
)]
async fn create(
    (State(app), ValidatedJson(input)): (State<AppState>, ValidatedJson<CreateArticleDto>),
) -> Result<Created<ArticleDto>> {
    Ok(Created(Envelope::created(
        ArticleService::create(app.db.db(), &input).await?,
    )))
}

#[utoipa::path(
    get,
    path = "/api/articles/{id}",
    params(("id" = String, Path, description = "文章 ID")),
    responses((status = 200, body = Envelope<ArticleDto>), (status = 404))
)]
async fn detail(
    (State(app), Path(id)): (State<AppState>, Path<String>),
) -> Result<Envelope<ArticleDto>> {
    Ok(Envelope::ok(ArticleService::get(app.db.db(), &id).await?))
}

#[utoipa::path(
    put,
    path = "/api/articles/{id}",
    params(("id" = String, Path, description = "文章 ID")),
    request_body = UpdateArticleDto,
    responses((status = 200, body = Envelope<ArticleDto>), (status = 404), (status = 422))
)]
async fn update(
    (State(app), Path(id), ValidatedJson(input)): (
        State<AppState>,
        Path<String>,
        ValidatedJson<UpdateArticleDto>,
    ),
) -> Result<Envelope<ArticleDto>> {
    Ok(Envelope::ok(
        ArticleService::update(app.db.db(), &id, &input).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/articles/{id}",
    params(("id" = String, Path, description = "文章 ID")),
    responses((status = 200, description = "已删除，data 为 null"), (status = 404))
)]
async fn remove(
    (State(app), Path(id)): (State<AppState>, Path<String>),
) -> Result<Envelope<serde_json::Value>> {
    ArticleService::remove(app.db.db(), &id).await?;
    Ok(Envelope::ok(serde_json::Value::Null))
}
