use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 内存数据库：线程安全的 HashMap
pub type Db = Arc<RwLock<HashMap<String, Todo>>>;

/// Todo 数据模型
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// 创建 Todo 的请求体
#[derive(Debug, Deserialize)]
pub struct CreateTodo {
    pub title: String,
}

/// 更新 Todo 的请求体
#[derive(Debug, Deserialize)]
pub struct UpdateTodo {
    pub title: Option<String>,
    pub completed: Option<bool>,
}

/// 分页参数
#[derive(Debug, Deserialize)]
pub struct Pagination {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}
