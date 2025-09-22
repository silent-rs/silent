use silent::prelude::*;

fn main() {
    logger::fmt().init();

    smol::block_on(async move {
        let route = Route::new("").get(|_req: Request| async move { Ok("ok") });

        // 在非 Tokio 运行时下，通过 async-compat 兼容内部 Tokio 后端
        async_compat::Compat::new(async move {
            Server::new().serve(route).await;
        })
        .await;
    });
}
