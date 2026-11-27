//! 运维端点：`/healthz` 报告进程活着，`/readyz` 报告能不能接流量。

use std::sync::Arc;

use serde_json::json;
use silent::extractor::State;
use silent::prelude::*;
use silent::{Response, StatusCode};

use super::error::Envelope;
use crate::db::DbRegistry;
use crate::db::migrate::AnyMigrator;

/// 就绪检查需要的东西：数据库连接和两组迁移器。
#[derive(Clone)]
pub struct Readiness {
    db: DbRegistry,
    framework: AnyMigrator,
    app: AnyMigrator,
}

impl Readiness {
    /// `framework` 是框架迁移器，`app` 是应用迁移器。
    pub fn new(
        db: DbRegistry,
        framework: impl Into<AnyMigrator>,
        app: impl Into<AnyMigrator>,
    ) -> Self {
        Self {
            db,
            framework: framework.into(),
            app: app.into(),
        }
    }

    /// 逐项检查，把不满足的原因都收齐。
    pub async fn check(&self) -> ReadinessReport {
        let mut reasons = Vec::new();
        if let Err(e) = self.db.ping_all().await {
            reasons.push(format!("database: {e}"));
            return ReadinessReport {
                ready: false,
                reasons,
            };
        }
        for (scope, migrator) in [("framework", &self.framework), ("app", &self.app)] {
            // 数据库比这个版本新（有这个版本不认识的迁移）不算未就绪：滚动发布时旧副本要继续服务。
            match migrator.diff(self.db.db()).await {
                Ok(diff) if !diff.pending.is_empty() => {
                    reasons.push(format!(
                        "{scope} migrations pending: {}",
                        diff.pending.len()
                    ));
                }
                Ok(_) => {}
                Err(e) => reasons.push(format!("{scope} migrations: {e}")),
            }
        }
        ReadinessReport {
            ready: reasons.is_empty(),
            reasons,
        }
    }
}

/// 就绪检查的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessReport {
    /// 能不能接流量。
    pub ready: bool,
    /// 不能接流量时的原因，能接流量时是空的。
    pub reasons: Vec<String>,
}

/// `/healthz` 和 `/readyz` 两条路由，各自是一个节点，直接挂在根路径（或 `base_path`）下。
///
/// `/readyz` 从 `State<Arc<Readiness>>` 取检查用的东西，所以挂载它的路由要用 `with_state`
/// 放进去一个 [`Readiness`]；[`crate::app::AppBuilder::build`] 会做这件事。
pub fn routes() -> Vec<Route> {
    vec![
        Route::new("healthz").get(healthz),
        Route::new("readyz").get(readyz),
    ]
}

/// 进程活着就返回 200。不查任何依赖，给 liveness 探针用。
async fn healthz(_req: Request) -> Result<Response> {
    Ok(Response::json(&json!({ "status": "ok" })))
}

/// 数据库连得上、没有待执行的迁移才返回 200；否则 503，`data.reasons` 里是原因。
async fn readyz(State(readiness): State<Arc<Readiness>>) -> Result<Response> {
    let report = readiness.check().await;
    if report.ready {
        return Ok(Response::json(&json!({ "status": "ok" })));
    }
    let envelope = Envelope::new(
        50_300,
        "service is not ready",
        json!({ "reasons": report.reasons }),
    );
    Ok(Response::json(&envelope).with_status(StatusCode::SERVICE_UNAVAILABLE))
}
