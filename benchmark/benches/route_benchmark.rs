use criterion::{Criterion, criterion_group, criterion_main};
use silent::prelude::*;

fn simple_route_benchmark(c: &mut Criterion) {
    let route = Route::new("").get(|_req: Request| async { Ok("hello world") });

    c.bench_function("simple route match", |b| {
        b.iter(|| {
            let req = Request::default();
            std::mem::drop(route.call(req));
        });
    });
}

fn nested_route_benchmark(c: &mut Criterion) {
    let route = Route::new("api/v1")
        .append(Route::new("users").get(|_req: Request| async { Ok("users") }))
        .append(Route::new("posts").get(|_req: Request| async { Ok("posts") }));

    c.bench_function("nested route match", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/posts".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });
}

fn middleware_route_benchmark(c: &mut Criterion) {
    let route = Route::new("")
        .hook(silent::middlewares::RequestTimeLogger)
        .get(|_req: Request| async { Ok("hello world") });

    c.bench_function("route with middleware", |b| {
        b.iter(|| {
            let req = Request::default();
            std::mem::drop(route.call(req));
        });
    });
}

fn complex_route_benchmark(c: &mut Criterion) {
    let route = Route::new("api/v1")
        .append(
            Route::new("users")
                .get(|_req: Request| async { Ok("users") })
                .post(|_req: Request| async { Ok("create user") }),
        )
        .append(
            Route::new("posts")
                .get(|_req: Request| async { Ok("posts") })
                .post(|_req: Request| async { Ok("create post") })
                .append(Route::new("comments").get(|_req: Request| async { Ok("comments") })),
        );

    let mut group = c.benchmark_group("Complex Routes");
    group.bench_function("GET /api/v1/users", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });
    group.bench_function("POST /api/v1/posts", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/posts".parse().unwrap();
            *req.method_mut() = Method::POST;
            std::mem::drop(route.call(req));
        });
    });
    group.bench_function("GET /api/v1/posts/comments", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/posts/comments".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });
    group.finish();
}

fn multiple_middleware_benchmark(c: &mut Criterion) {
    let route = Route::new("")
        .hook(silent::middlewares::RequestTimeLogger)
        .hook(silent::middlewares::RequestTimeLogger) // Using same middleware twice for testing
        .get(|_req: Request| async { Ok("hello world") });

    c.bench_function("route with multiple middleware", |b| {
        b.iter(|| {
            let req = Request::default();
            std::mem::drop(route.call(req));
        });
    });
}

fn high_load_benchmark(c: &mut Criterion) {
    let route = Route::new("api/v1").append(
        Route::new("users")
            .get(|_req: Request| async { Ok("users") })
            .post(|_req: Request| async { Ok("create user") }),
    );

    let mut group = c.benchmark_group("High Load");
    group.sample_size(1000);
    group.bench_function("1000 sequential requests", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let mut req = Request::default();
                *req.uri_mut() = format!("/api/v1/users?page={}", i).parse().unwrap();
                std::mem::drop(route.call(req));
            }
        });
    });
    group.finish();
}

fn deep_nested_route_benchmark(c: &mut Criterion) {
    // 创建10层嵌套的复杂路由结构
    let route = Route::new("api/v1").append(
        Route::new("users").append(
            Route::new("profiles").append(
                Route::new("settings").append(
                    Route::new("preferences").append(
                        Route::new("notifications").append(
                            Route::new("email").append(
                                Route::new("templates").append(
                                    Route::new("custom")
                                        .append(Route::new("advanced").get(
                                            |_req: Request| async { Ok("deep nested route") },
                                        )),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    );

    let mut group = c.benchmark_group("Deep Nested Routes (10 levels)");

    // 测试匹配到最深层的路由
    group.bench_function("match deepest route", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email/templates/custom/advanced".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    // 测试匹配中间层的路由
    group.bench_function("match middle level route", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications"
                .parse()
                .unwrap();
            std::mem::drop(route.call(req));
        });
    });

    // 测试不匹配的路由
    group.bench_function("unmatched route", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email/templates/custom/nonexistent".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    group.finish();
}

fn complex_deep_route_with_params_benchmark(c: &mut Criterion) {
    // 创建包含路径参数的10层复杂路由
    let route = Route::new("api/v1").append(Route::new("users/<user_id:i64>").append(
        Route::new("profiles/<profile_id>").append(
            Route::new("settings/<setting_type>").append(
                Route::new("preferences/<pref_category>").append(
                    Route::new("notifications/<notif_type>").append(
                        Route::new("email/<email_id>").append(
                            Route::new("templates/<template_id>").append(
                                Route::new("custom/<custom_id>").append(
                                    Route::new("advanced/<advanced_param>").get(
                                        |_req: Request| async { Ok("deep nested with params") },
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    ));

    let mut group = c.benchmark_group("Deep Nested Routes with Parameters (10 levels)");

    // 测试匹配到最深层的路由（带参数）
    group.bench_function("match deepest route with params", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/123/profiles/profile_456/settings/email/preferences/security/notifications/push/email/email_789/templates/template_101/custom/custom_202/advanced/advanced_303".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    // 测试匹配中间层的路由（带参数）
    group.bench_function("match middle level route with params", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() =
                "/api/v1/users/456/profiles/profile_789/settings/notification/preferences/privacy"
                    .parse()
                    .unwrap();
            std::mem::drop(route.call(req));
        });
    });

    group.finish();
}

fn deep_route_with_middleware_benchmark(c: &mut Criterion) {
    // 创建带中间件的10层复杂路由
    let route = Route::new("api/v1")
        .hook(silent::middlewares::RequestTimeLogger)
        .append(
            Route::new("users")
                .hook(silent::middlewares::RequestTimeLogger)
                .append(
                    Route::new("profiles")
                        .hook(silent::middlewares::RequestTimeLogger)
                        .append(
                            Route::new("settings")
                                .hook(silent::middlewares::RequestTimeLogger)
                                .append(
                                    Route::new("preferences")
                                        .hook(silent::middlewares::RequestTimeLogger)
                                        .append(
                                            Route::new("notifications")
                                                .hook(silent::middlewares::RequestTimeLogger)
                                                .append(
                                                    Route::new("email")
                                                        .hook(silent::middlewares::RequestTimeLogger)
                                                        .append(
                                                            Route::new("templates")
                                                                .hook(silent::middlewares::RequestTimeLogger)
                                                                .append(
                                                                    Route::new("custom")
                                                                        .hook(silent::middlewares::RequestTimeLogger)
                                                                        .append(
                                                                            Route::new("advanced")
                                                                                .hook(silent::middlewares::RequestTimeLogger)
                                                                                .get(|_req: Request| async { Ok("deep nested with middleware") })
                                                                        )
                                                                )
                                                        )
                                                )
                                        )
                                )
                        )
                )
        );

    let mut group = c.benchmark_group("Deep Nested Routes with Middleware (10 levels)");

    // 测试匹配到最深层的路由（带中间件）
    group.bench_function("match deepest route with middleware", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email/templates/custom/advanced".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    group.finish();
}

fn deep_route_mixed_benchmark(c: &mut Criterion) {
    // 创建混合了静态路径、参数路径和中间件的10层复杂路由
    let route = Route::new("api/v1")
        .hook(silent::middlewares::RequestTimeLogger)
        .append(
            Route::new("users/<user_id:i64>")
                .hook(silent::middlewares::RequestTimeLogger)
                .append(
                    Route::new("profiles")
                        .append(
                            Route::new("settings/<setting_id>")
                                .hook(silent::middlewares::RequestTimeLogger)
                                .append(
                                    Route::new("preferences")
                                        .append(
                                            Route::new("notifications/<notif_type>")
                                                .hook(silent::middlewares::RequestTimeLogger)
                                                .append(
                                                    Route::new("email")
                                                        .append(
                                                            Route::new("templates/<template_id:i64>")
                                                                .hook(silent::middlewares::RequestTimeLogger)
                                                                .append(
                                                                    Route::new("custom")
                                                                        .append(
                                                                            Route::new("advanced/<advanced_param>")
                                                                                .hook(silent::middlewares::RequestTimeLogger)
                                                                                .get(|_req: Request| async { Ok("mixed deep nested") })
                                                                        )
                                                                )
                                                        )
                                                )
                                        )
                                )
                        )
                )
        );

    let mut group = c.benchmark_group("Mixed Deep Nested Routes (10 levels)");

    // 测试匹配到最深层的路由（混合类型）
    group.bench_function("match deepest mixed route", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/123/profiles/settings/setting_456/preferences/notifications/push/email/templates/789/custom/advanced/advanced_param".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    // 测试匹配中间层的路由（混合类型）
    group.bench_function("match middle mixed route", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() =
                "/api/v1/users/456/profiles/settings/setting_789/preferences/notifications/email"
                    .parse()
                    .unwrap();
            std::mem::drop(route.call(req));
        });
    });

    group.finish();
}

fn route_matching_only_benchmark(c: &mut Criterion) {
    // 创建10层嵌套的复杂路由结构，只测试匹配性能，不调用handler
    let route = Route::new("api/v1").append(
        Route::new("users").append(
            Route::new("profiles").append(
                Route::new("settings").append(
                    Route::new("preferences").append(
                        Route::new("notifications").append(
                            Route::new("email").append(
                                Route::new("templates").append(
                                    Route::new("custom")
                                        .append(Route::new("advanced").get(
                                            |_req: Request| async { Ok("deep nested route") },
                                        )),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    );

    let mut group = c.benchmark_group("Route Matching Only (10 levels)");

    // 只测试路由匹配，不调用handler
    group.bench_function("match deepest route (no handler call)", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email/templates/custom/advanced".parse().unwrap();
            // 直接调用路由，但不等待结果
            std::mem::drop(route.call(req));
        });
    });

    // 测试匹配中间层
    group.bench_function("match middle level route (no handler call)", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications"
                .parse()
                .unwrap();
            // 直接调用路由，但不等待结果
            std::mem::drop(route.call(req));
        });
    });

    // 测试不匹配的情况
    group.bench_function("unmatched route (no handler call)", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email/templates/custom/nonexistent".parse().unwrap();
            // 直接调用路由，但不等待结果
            std::mem::drop(route.call(req));
        });
    });

    group.finish();
}

fn route_matching_with_params_only_benchmark(c: &mut Criterion) {
    // 创建包含路径参数的10层复杂路由，只测试匹配性能
    let route = Route::new("api/v1").append(Route::new("users/<user_id:i64>").append(
        Route::new("profiles/<profile_id>").append(
            Route::new("settings/<setting_type>").append(
                Route::new("preferences/<pref_category>").append(
                    Route::new("notifications/<notif_type>").append(
                        Route::new("email/<email_id>").append(
                            Route::new("templates/<template_id>").append(
                                Route::new("custom/<custom_id>").append(
                                    Route::new("advanced/<advanced_param>").get(
                                        |_req: Request| async { Ok("deep nested with params") },
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    ));

    let mut group = c.benchmark_group("Route Matching with Parameters Only (10 levels)");

    // 只测试路由匹配，不调用handler
    group.bench_function("match deepest route with params (no handler call)", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/123/profiles/profile_456/settings/email/preferences/security/notifications/push/email/email_789/templates/template_101/custom/custom_202/advanced/advanced_303".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    // 测试匹配中间层
    group.bench_function("match middle level route with params (no handler call)", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/456/profiles/profile_789/settings/notification/preferences/privacy".parse().unwrap();
            std::mem::drop(route.call(req));
        });
    });

    group.finish();
}

fn route_matching_performance_comparison_benchmark(c: &mut Criterion) {
    // 创建不同深度的路由进行性能对比
    let route_3_levels = Route::new("api/v1").append(
        Route::new("users")
            .append(Route::new("profiles").get(|_req: Request| async { Ok("3 levels") })),
    );

    let route_5_levels =
        Route::new("api/v1").append(Route::new("users").append(
            Route::new("profiles").append(
                Route::new("settings").append(
                    Route::new("preferences").get(|_req: Request| async { Ok("5 levels") }),
                ),
            ),
        ));

    let route_7_levels =
        Route::new("api/v1").append(Route::new("users").append(
            Route::new("profiles").append(
                Route::new("settings").append(
                    Route::new("preferences").append(
                        Route::new("notifications").append(
                            Route::new("email").get(|_req: Request| async { Ok("7 levels") }),
                        ),
                    ),
                ),
            ),
        ));

    let route_10_levels =
        Route::new("api/v1").append(Route::new("users").append(Route::new("profiles").append(
            Route::new("settings").append(Route::new("preferences").append(
                Route::new("notifications").append(Route::new("email").append(
                    Route::new("templates").append(Route::new("custom").append(
                        Route::new("advanced").get(|_req: Request| async { Ok("10 levels") }),
                    )),
                )),
            )),
        )));

    let mut group = c.benchmark_group("Route Matching Performance Comparison");

    // 测试3层路由匹配
    group.bench_function("3 levels route match", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles".parse().unwrap();
            std::mem::drop(route_3_levels.call(req));
        });
    });

    // 测试5层路由匹配
    group.bench_function("5 levels route match", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences"
                .parse()
                .unwrap();
            std::mem::drop(route_5_levels.call(req));
        });
    });

    // 测试7层路由匹配
    group.bench_function("7 levels route match", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email"
                .parse()
                .unwrap();
            std::mem::drop(route_7_levels.call(req));
        });
    });

    // 测试10层路由匹配
    group.bench_function("10 levels route match", |b| {
        b.iter(|| {
            let mut req = Request::default();
            *req.uri_mut() = "/api/v1/users/profiles/settings/preferences/notifications/email/templates/custom/advanced".parse().unwrap();
            std::mem::drop(route_10_levels.call(req));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    simple_route_benchmark,
    nested_route_benchmark,
    middleware_route_benchmark,
    complex_route_benchmark,
    multiple_middleware_benchmark,
    high_load_benchmark,
    deep_nested_route_benchmark,
    complex_deep_route_with_params_benchmark,
    deep_route_with_middleware_benchmark,
    deep_route_mixed_benchmark,
    route_matching_only_benchmark,
    route_matching_with_params_only_benchmark,
    route_matching_performance_comparison_benchmark
);
criterion_main!(benches);
