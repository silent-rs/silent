use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use silent::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

fn main() {
    logger::fmt().init();
    let db = Db::default();
    let middle_ware = MiddleWare { db };
    let route = Route::new("todos")
        .hook(middle_ware)
        .get(todos_index)
        .post(todos_create)
        .append(
            Route::new("<id:uuid>")
                .get(todos_one)
                .patch(todos_update)
                .delete(todos_delete),
        );
    Server::new().run(route);
}

struct MiddleWare {
    db: Db,
}

#[async_trait]
impl MiddleWareHandler for MiddleWare {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        req.extensions_mut().insert(self.db.clone());
        next.call(req).await
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct Pagination {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

async fn todos_index(mut req: Request) -> Result<Vec<Todo>> {
    let pagination = req.params_parse::<Pagination>()?;

    let db = req.extensions().get::<Db>().unwrap();
    let todos = db.read().unwrap();

    let todos = todos
        .values()
        .skip(pagination.offset.unwrap_or(0))
        .take(pagination.limit.unwrap_or(usize::MAX))
        .cloned()
        .collect::<Vec<_>>();

    Ok(todos)
}

#[derive(Debug, Deserialize, Serialize)]
struct CreateTodo {
    text: String,
}

async fn todos_create(mut req: Request) -> Result<Todo> {
    let create_todo = req.json_parse::<CreateTodo>().await?;
    let db = req.extensions().get::<Db>().unwrap();

    let todo = Todo {
        id: Uuid::new_v4(),
        text: create_todo.text,
        completed: false,
    };

    db.write().unwrap().insert(todo.id, todo.clone());

    Ok(todo)
}

#[derive(Debug, Deserialize, Serialize)]
struct UpdateTodo {
    text: Option<String>,
    completed: Option<bool>,
}

async fn todos_update(mut req: Request) -> Result<Todo> {
    let input = req.json_parse::<UpdateTodo>().await?;
    let db = req.extensions().get::<Db>().unwrap();
    let id: Uuid = req.get_path_params("id")?;
    let todo = db.read().unwrap().get(&id).cloned();

    if todo.is_none() {
        return Err(SilentError::BusinessError {
            code: StatusCode::NOT_FOUND,
            msg: "Not Found".to_string(),
        });
    }

    let mut todo = todo.unwrap();

    if let Some(text) = input.text {
        todo.text = text;
    }

    if let Some(completed) = input.completed {
        todo.completed = completed;
    }

    db.write().unwrap().insert(todo.id, todo.clone());

    Ok(todo)
}

async fn todos_one(req: Request) -> Result<Todo> {
    let db = req.extensions().get::<Db>().unwrap();
    let id: Uuid = req.get_path_params("id")?;
    let todo = db.read().unwrap().get(&id).cloned();

    if todo.is_none() {
        return Err(SilentError::BusinessError {
            code: StatusCode::NOT_FOUND,
            msg: "Not Found".to_string(),
        });
    }

    let todo = todo.unwrap();
    Ok(todo)
}

async fn todos_delete(req: Request) -> Result<()> {
    let db = req.extensions().get::<Db>().unwrap();
    let id = req.get_path_params("id")?;
    if db.write().unwrap().remove(&id).is_some() {
        Ok(())
    } else {
        Err(SilentError::BusinessError {
            code: StatusCode::NOT_FOUND,
            msg: "Not Found".to_string(),
        })
    }
}

type Db = Arc<RwLock<HashMap<Uuid, Todo>>>;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Todo {
    id: Uuid,
    text: String,
    completed: bool,
}
