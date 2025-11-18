use async_trait::async_trait;
use silent::prelude::*;
use std::sync::atomic::AtomicUsize;

fn main() {
    logger::fmt().init();
    let middle_ware = MiddleWare {
        count: AtomicUsize::new(0),
    };
    let route = Route::new("")
        .hook(middle_ware)
        .get(|_req: Request| async { Ok("Hello World") });
    Server::new().run(route);
}

struct MiddleWare {
    count: AtomicUsize,
}

#[async_trait]
impl MiddleWareHandler for MiddleWare {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let count = self.count.load(std::sync::atomic::Ordering::SeqCst);
        info!("pre_request count: {}", count);
        if count.is_multiple_of(2) {
            error!("set pre_request error");
            return Err(SilentError::BusinessError {
                code: StatusCode::BAD_REQUEST,
                msg: "bad request:pre_request".to_string(),
            });
        }
        next.call(req).await
    }
}
