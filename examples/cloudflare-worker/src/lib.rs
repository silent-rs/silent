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

    // 将 Env 和 Context 注入到 Configs 中
    // 处理器可通过 req.get_config::<Env>() 和 req.get_config::<Context>() 获取
    let mut cfg = Configs::default();
    cfg.insert(env);
    cfg.insert(ctx);

    let wr = get_route().with_configs(cfg);

    Ok(wr.call(req).await)
}
