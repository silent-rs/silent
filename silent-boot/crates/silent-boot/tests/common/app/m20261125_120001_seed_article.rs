use silent_boot::db::migrate::*;

/// 往 article 表写入 `count` 行样例数据。主键是随机串，重复执行不会因为主键冲突而失败。
pub async fn seed_article<C: ConnectionTrait>(db: &C, count: usize) -> Result<(), DbErr> {
    for i in 0..count {
        let now = chrono::Local::now().naive_local();
        let stmt = Query::insert()
            .into_table(Alias::new("article"))
            .columns([
                Alias::new("id"),
                Alias::new("title"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                scru128::new_string().into(),
                format!("article-{i}").into(),
                now.into(),
                now.into(),
            ])
            .to_owned();
        db.execute(&stmt).await?;
    }
    Ok(())
}

silent_boot::migration! {
    /// 写入两行样例数据。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        super::seed_delay().await;
        seed_article(manager.get_connection(), 2).await
    }
}
