use crate::model::{CreateTodo, Db, Pagination, Todo, UpdateTodo};
use silent::prelude::*;

/// 构建 API 路由
pub fn api_routes() -> Route {
    Route::new("api/todos")
        .get(list_todos)
        .post(create_todo)
        .append(
            Route::new("<id>")
                .get(get_todo)
                .put(update_todo)
                .delete(delete_todo),
        )
}

/// GET /api/todos — 获取 Todo 列表（支持分页）
async fn list_todos(mut req: Request) -> Result<Response> {
    let pagination = req.params_parse::<Pagination>()?;
    let db = req.get_config::<Db>()?;
    let todos = db.read().unwrap();

    let mut list: Vec<&Todo> = todos.values().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let list: Vec<&Todo> = list
        .into_iter()
        .skip(pagination.offset.unwrap_or(0))
        .take(pagination.limit.unwrap_or(20))
        .collect();

    Ok(Response::json(&serde_json::json!({
        "total": todos.len(),
        "items": list,
    })))
}

/// POST /api/todos — 创建 Todo
async fn create_todo(mut req: Request) -> Result<Response> {
    let input: CreateTodo = req.json_parse().await?;

    if input.title.trim().is_empty() {
        return Err(SilentError::business_error(
            StatusCode::BAD_REQUEST,
            "title 不能为空".to_string(),
        ));
    }

    let db = req.get_config::<Db>()?.clone();
    let now = chrono::Local::now().naive_local();
    let todo = Todo {
        id: scru128::new_string(),
        title: input.title.trim().to_string(),
        completed: false,
        created_at: now,
        updated_at: now,
    };

    db.write().unwrap().insert(todo.id.clone(), todo.clone());

    Ok(Response::json(&todo).with_status(StatusCode::CREATED))
}

/// GET /api/todos/:id — 获取单个 Todo
async fn get_todo(req: Request) -> Result<Response> {
    let id: String = req.get_path_params("id")?;
    let db = req.get_config::<Db>()?;
    let todos = db.read().unwrap();

    let todo = todos.get(&id).ok_or_else(|| {
        SilentError::business_error(StatusCode::NOT_FOUND, format!("todo '{id}' 不存在"))
    })?;

    Ok(Response::json(todo))
}

/// PUT /api/todos/:id — 更新 Todo
async fn update_todo(mut req: Request) -> Result<Response> {
    let id: String = req.get_path_params("id")?;
    let input: UpdateTodo = req.json_parse().await?;
    let db = req.get_config::<Db>()?.clone();

    let mut todos = db.write().unwrap();
    let todo = todos.get_mut(&id).ok_or_else(|| {
        SilentError::business_error(StatusCode::NOT_FOUND, format!("todo '{id}' 不存在"))
    })?;

    if let Some(title) = &input.title {
        if title.trim().is_empty() {
            return Err(SilentError::business_error(
                StatusCode::BAD_REQUEST,
                "title 不能为空".to_string(),
            ));
        }
        todo.title = title.trim().to_string();
    }

    if let Some(completed) = input.completed {
        todo.completed = completed;
    }

    todo.updated_at = chrono::Local::now().naive_local();

    Ok(Response::json(todo))
}

/// DELETE /api/todos/:id — 删除 Todo
async fn delete_todo(req: Request) -> Result<Response> {
    let id: String = req.get_path_params("id")?;
    let db = req.get_config::<Db>()?;

    db.write().unwrap().remove(&id).ok_or_else(|| {
        SilentError::business_error(StatusCode::NOT_FOUND, format!("todo '{id}' 不存在"))
    })?;

    Ok(Response::json(&serde_json::json!({
        "status": "deleted",
        "id": id,
    })))
}
