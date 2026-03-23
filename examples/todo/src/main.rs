//! Todo REST API 示例
//!
//! 展示 Silent 框架的最佳实践：
//! - 路由组织与嵌套
//! - Logger / ExceptionHandler 中间件
//! - 提取器（Path / Json / Query）
//! - State 注入共享状态
//! - 结构化 JSON 错误响应
//! - 完整 CRUD 操作
//!
//! ## API 端点
//!
//! | 方法   | 路径            | 说明           |
//! |--------|-----------------|----------------|
//! | GET    | /api/todos      | 列表（支持分页）|
//! | POST   | /api/todos      | 创建           |
//! | GET    | /api/todos/:id  | 详情           |
//! | PUT    | /api/todos/:id  | 更新           |
//! | DELETE | /api/todos/:id  | 删除           |
//!
//! ## 测试
//!
//! ```bash
//! # 创建
//! curl -X POST http://localhost:8080/api/todos \
//!   -H "Content-Type: application/json" \
//!   -d '{"title":"买菜"}'
//!
//! # 列表
//! curl http://localhost:8080/api/todos
//! curl "http://localhost:8080/api/todos?offset=0&limit=10"
//!
//! # 详情（替换 ID）
//! curl http://localhost:8080/api/todos/<id>
//!
//! # 更新
//! curl -X PUT http://localhost:8080/api/todos/<id> \
//!   -H "Content-Type: application/json" \
//!   -d '{"title":"买水果","completed":true}'
//!
//! # 删除
//! curl -X DELETE http://localhost:8080/api/todos/<id>
//! ```

mod model;
mod route;

use silent::middlewares::{ExceptionHandler, Logger};
use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();

    // 初始化内存数据库并通过 with_state 注入
    let db = model::Db::default();

    // 构建路由
    let route = Route::new_root()
        .hook(Logger::new())
        .hook(ExceptionHandler::new(
            |result: Result<Response>, _state| async move {
                match result {
                    Ok(res) => Ok(res),
                    Err(e) => {
                        let status = e.status();
                        Ok(Response::json(&serde_json::json!({
                            "error": e.to_string(),
                            "code": status.as_u16(),
                        }))
                        .with_status(status))
                    }
                }
            },
        ))
        .append(route::api_routes())
        .with_state(db);

    Server::new().run(route);
}
