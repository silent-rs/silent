use silent::prelude::*;
use silent::service::transport::AsyncIoTransport;

fn main() {
    logger::fmt().init();

    smol::block_on(async move {
        let route = Route::new("").get(|_req: Request| async move { Ok("ok") });
        // 选择 AsyncIoTransport，完全不依赖 tokio
        Server::new()
            .with_transport(AsyncIoTransport::new())
            .serve(route)
            .await;
    });
}
