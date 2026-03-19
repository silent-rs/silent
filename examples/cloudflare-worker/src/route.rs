use http_body_util::BodyExt;
use silent::{
    Request, Response,
    prelude::{Route, WorkRoute},
};
use worker::Env;

pub fn get_route() -> WorkRoute {
    let route = Route::new_root()
        // 基础路由
        .append(Route::new("hello").get(hello_handler))
        // KV 示例
        .append(
            Route::new("kv").append(
                Route::new("<key>")
                    .get(kv_get)
                    .put(kv_put)
                    .delete(kv_delete),
            ),
        )
        // D1 示例
        .append(
            Route::new("d1/users")
                .get(d1_list_users)
                .post(d1_create_user),
        )
        // R2 示例
        .append(
            Route::new("r2").append(
                Route::new("<key>")
                    .get(r2_get)
                    .put(r2_put)
                    .delete(r2_delete),
            ),
        );

    WorkRoute::new(route)
}

/// 从请求体读取全部字节
async fn read_body_bytes(req: &mut Request) -> silent::Result<Vec<u8>> {
    let body = req.take_body();
    let collected = body.collect().await.map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            format!("read body error: {e}"),
        )
    })?;
    Ok(collected.to_bytes().to_vec())
}

/// 从请求体读取文本
async fn read_body_text(req: &mut Request) -> silent::Result<String> {
    let bytes = read_body_bytes(req).await?;
    String::from_utf8(bytes).map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            format!("invalid UTF-8: {e}"),
        )
    })
}

/// Worker 错误转 SilentError
fn worker_err(msg: impl std::fmt::Display) -> silent::SilentError {
    silent::SilentError::business_error(silent::StatusCode::INTERNAL_SERVER_ERROR, msg.to_string())
}

// ==================== 基础处理器 ====================

async fn hello_handler(_req: Request) -> silent::Result<&'static str> {
    Ok("hello from Cloudflare Worker via Silent")
}

// ==================== KV 处理器 ====================

/// GET /kv/:key — 从 KV 读取值
async fn kv_get(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;

    let kv = env.kv("MY_KV").map_err(worker_err)?;

    match kv.get(&key).text().await {
        Ok(Some(value)) => Ok(Response::json(&serde_json::json!({
            "key": key,
            "value": value,
        }))),
        Ok(None) => Err(silent::SilentError::business_error(
            silent::StatusCode::NOT_FOUND,
            format!("key '{key}' not found"),
        )),
        Err(e) => Err(worker_err(format!("KV get error: {e}"))),
    }
}

/// PUT /kv/:key — 写入 KV（请求体为值）
async fn kv_put(mut req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?.clone();
    let key: String = req.get_path_params("key")?;
    let value = read_body_text(&mut req).await?;

    let kv = env.kv("MY_KV").map_err(worker_err)?;

    kv.put(&key, &value)
        .map_err(worker_err)?
        .execute()
        .await
        .map_err(worker_err)?;

    Ok(Response::json(&serde_json::json!({
        "status": "ok",
        "key": key,
    })))
}

/// DELETE /kv/:key — 删除 KV 键
async fn kv_delete(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;

    let kv = env.kv("MY_KV").map_err(worker_err)?;
    kv.delete(&key).await.map_err(worker_err)?;

    Ok(Response::json(&serde_json::json!({
        "status": "deleted",
        "key": key,
    })))
}

// ==================== D1 处理器 ====================

/// GET /d1/users — 查询 D1 用户表
async fn d1_list_users(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;

    let d1 = env.d1("MY_DB").map_err(worker_err)?;
    let stmt = d1.prepare("SELECT id, name, email FROM users LIMIT 100");
    let result = stmt.all().await.map_err(worker_err)?;
    let rows: Vec<serde_json::Value> = result.results().map_err(worker_err)?;

    Ok(Response::json(&serde_json::json!({ "users": rows })))
}

/// POST /d1/users — 创建用户（JSON 请求体: { "name": "...", "email": "..." }）
async fn d1_create_user(mut req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?.clone();
    let body: serde_json::Value = req.json_parse().await?;

    let name = body["name"].as_str().ok_or_else(|| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            "missing field: name".to_string(),
        )
    })?;

    let email = body["email"].as_str().ok_or_else(|| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            "missing field: email".to_string(),
        )
    })?;

    let d1 = env.d1("MY_DB").map_err(worker_err)?;

    d1.prepare("INSERT INTO users (name, email) VALUES (?, ?)")
        .bind(&[name.into(), email.into()])
        .map_err(worker_err)?
        .run()
        .await
        .map_err(worker_err)?;

    Ok(Response::json(&serde_json::json!({
        "status": "created",
        "name": name,
        "email": email,
    })))
}

// ==================== R2 处理器 ====================

/// GET /r2/:key — 从 R2 读取对象
async fn r2_get(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;

    let bucket = env.bucket("MY_BUCKET").map_err(worker_err)?;

    match bucket.get(&key).execute().await {
        Ok(Some(obj)) => {
            let body = obj
                .body()
                .ok_or_else(|| worker_err("R2 object has no body"))?
                .bytes()
                .await
                .map_err(worker_err)?;

            let mut resp = Response::empty();
            resp.set_body(silent::prelude::ResBody::from(body));
            Ok(resp)
        }
        Ok(None) => Err(silent::SilentError::business_error(
            silent::StatusCode::NOT_FOUND,
            format!("object '{key}' not found"),
        )),
        Err(e) => Err(worker_err(format!("R2 get error: {e}"))),
    }
}

/// PUT /r2/:key — 上传对象到 R2（请求体为文件内容）
async fn r2_put(mut req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?.clone();
    let key: String = req.get_path_params("key")?;
    let body = read_body_bytes(&mut req).await?;

    let bucket = env.bucket("MY_BUCKET").map_err(worker_err)?;
    bucket.put(&key, body).execute().await.map_err(worker_err)?;

    Ok(Response::json(&serde_json::json!({
        "status": "uploaded",
        "key": key,
    })))
}

/// DELETE /r2/:key — 删除 R2 对象
async fn r2_delete(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;

    let bucket = env.bucket("MY_BUCKET").map_err(worker_err)?;
    bucket.delete(&key).await.map_err(worker_err)?;

    Ok(Response::json(&serde_json::json!({
        "status": "deleted",
        "key": key,
    })))
}
