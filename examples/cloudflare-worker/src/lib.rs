#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod route;
#[cfg(target_arch = "wasm32")]
use worker::{Context, Env, Request, Response, Result};

#[cfg(target_arch = "wasm32")]
use crate::route::get_route;

#[cfg(target_arch = "wasm32")]
#[worker::event(fetch)]
pub async fn main(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    Ok(get_route().call(req).await)
}
