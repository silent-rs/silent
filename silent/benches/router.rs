use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use silent::Handler;
use silent::prelude::{Method, Request, Route, SilentError};
use tokio::runtime::Runtime;

const ROUTE_SIZES: &[usize] = &[32, 128, 512, 1024, 2048];
const TARGET_PATH: &str = "/users/123/orders/999";

async fn ok(_: Request) -> Result<&'static str, SilentError> {
    Ok("ok")
}

fn router_benchmarks(c: &mut Criterion) {
    for &route_count in ROUTE_SIZES {
        assert!(
            route_count >= 1,
            "route benchmark requires at least one route"
        );

        let mut root = Route::new("");
        for idx in 0..route_count.saturating_sub(1) {
            let path = format!("service{idx}/health");
            root = root.append(Route::new(&path).get(ok));
        }
        root = root.append(Route::new("users/<user_id:int>/orders/<order_id:int>").get(ok));

        let tree = Arc::new(root.into_route_tree());
        let bench_id = format!("router_match/{route_count}");

        c.bench_function(&bench_id, |b| {
            let runtime = Runtime::new().expect("create runtime");
            let tree = Arc::clone(&tree);
            b.iter(|| {
                let tree = Arc::clone(&tree);
                runtime.block_on(async move {
                    let mut req = Request::empty();
                    *req.method_mut() = Method::GET;
                    *req.uri_mut() = TARGET_PATH.parse().expect("static path");
                    tree.call(req).await.expect("route matched");
                });
            });
        });
    }
}

criterion_group!(router, router_benchmarks);
criterion_main!(router);
