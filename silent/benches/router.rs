use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use silent::Handler;
use silent::prelude::{Method, Request, Route, SilentError};
use tokio::runtime::Runtime;

const ROUTE_SIZES: &[usize] = &[32, 128, 512, 1024, 2048];

async fn ok(_: Request) -> Result<&'static str, SilentError> {
    Ok("ok")
}

/// 原有 benchmark：不同规模路由表下的参数路由匹配
fn router_benchmarks(c: &mut Criterion) {
    let target_path = "/users/123/orders/999";

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
                    *req.uri_mut() = target_path.parse().expect("static path");
                    tree.call(req).await.expect("route matched");
                });
            });
        });
    }
}

/// 纯静态路由匹配（最佳路径，HashMap 查找）
fn static_route_benchmarks(c: &mut Criterion) {
    let mut root = Route::new("");
    for i in 0..100 {
        root = root.append(Route::new(&format!("api/v1/resource{i}")).get(ok));
    }
    let tree = Arc::new(root.into_route_tree());

    // 匹配第50个路由
    c.bench_function("static_match/100_routes", |b| {
        let runtime = Runtime::new().expect("create runtime");
        let tree = Arc::clone(&tree);
        b.iter(|| {
            let tree = Arc::clone(&tree);
            runtime.block_on(async move {
                let mut req = Request::empty();
                *req.method_mut() = Method::GET;
                *req.uri_mut() = "/api/v1/resource50".parse().unwrap();
                tree.call(req).await.expect("route matched");
            });
        });
    });
}

/// 参数路由匹配（单个参数 vs 多个参数）
fn param_route_benchmarks(c: &mut Criterion) {
    let root = Route::new("")
        .append(Route::new("users/<id>").get(ok))
        .append(Route::new("users/<user_id>/posts/<post_id>/comments/<comment_id>").get(ok));
    let tree = Arc::new(root.into_route_tree());

    c.bench_function("param_match/single", |b| {
        let runtime = Runtime::new().expect("create runtime");
        let tree = Arc::clone(&tree);
        b.iter(|| {
            let tree = Arc::clone(&tree);
            runtime.block_on(async move {
                let mut req = Request::empty();
                *req.method_mut() = Method::GET;
                *req.uri_mut() = "/users/42".parse().unwrap();
                tree.call(req).await.expect("route matched");
            });
        });
    });

    c.bench_function("param_match/triple", |b| {
        let runtime = Runtime::new().expect("create runtime");
        let tree = Arc::clone(&tree);
        b.iter(|| {
            let tree = Arc::clone(&tree);
            runtime.block_on(async move {
                let mut req = Request::empty();
                *req.method_mut() = Method::GET;
                *req.uri_mut() = "/users/1/posts/2/comments/3".parse().unwrap();
                tree.call(req).await.expect("route matched");
            });
        });
    });
}

/// 深层嵌套路由
fn deep_nested_benchmarks(c: &mut Criterion) {
    let root =
        Route::new("").append(Route::new("api").append(Route::new("v1").append(
            Route::new("organizations").append(
                Route::new("<org_id>").append(
                    Route::new("teams").append(
                        Route::new("<team_id>").append(
                            Route::new("members").append(Route::new("<member_id>").get(ok)),
                        ),
                    ),
                ),
            ),
        )));
    let tree = Arc::new(root.into_route_tree());

    c.bench_function("deep_nested/7_levels", |b| {
        let runtime = Runtime::new().expect("create runtime");
        let tree = Arc::clone(&tree);
        b.iter(|| {
            let tree = Arc::clone(&tree);
            runtime.block_on(async move {
                let mut req = Request::empty();
                *req.method_mut() = Method::GET;
                *req.uri_mut() = "/api/v1/organizations/org1/teams/team2/members/mem3"
                    .parse()
                    .unwrap();
                tree.call(req).await.expect("route matched");
            });
        });
    });
}

/// 不匹配路径（最差情况）
fn not_found_benchmarks(c: &mut Criterion) {
    let mut root = Route::new("");
    for i in 0..100 {
        root = root.append(Route::new(&format!("api/v1/resource{i}")).get(ok));
    }
    let tree = Arc::new(root.into_route_tree());

    c.bench_function("not_found/100_routes", |b| {
        let runtime = Runtime::new().expect("create runtime");
        let tree = Arc::clone(&tree);
        b.iter(|| {
            let tree = Arc::clone(&tree);
            runtime.block_on(async move {
                let mut req = Request::empty();
                *req.method_mut() = Method::GET;
                *req.uri_mut() = "/nonexistent/path".parse().unwrap();
                let _ = tree.call(req).await;
            });
        });
    });
}

criterion_group!(
    router,
    router_benchmarks,
    static_route_benchmarks,
    param_route_benchmarks,
    deep_nested_benchmarks,
    not_found_benchmarks,
);
criterion_main!(router);
