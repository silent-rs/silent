#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod route;
#[cfg(target_arch = "wasm32")]
use worker::{Context, Env, Request, Response, Result};

#[cfg(target_arch = "wasm32")]
use crate::route::get_route;
#[cfg(target_arch = "wasm32")]
use silent::Configs;

#[cfg(target_arch = "wasm32")]
#[worker::event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // 将 Env 注入到 Configs 中，处理器可通过 req.get_config::<Env>() 获取
    // 然后在处理器中按需获取 KV/D1/R2 等绑定
    let mut cfg = Configs::default();
    cfg.insert(env);

    // 将 Context 放入 Request extensions（每次请求独立，不适合放入 Configs）
    let wr = get_route().with_configs(cfg);

    Ok(wr.call(req).await)
}
