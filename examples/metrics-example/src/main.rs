use metrics_exporter_prometheus::PrometheusBuilder;
use silent::prelude::*;

fn init_metrics() {
    PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], 9898))
        .install()
        .expect("install prometheus recorder");
}

#[tokio::main]
async fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    init_metrics();

    let route = Route::new("").get(|_req: Request| async { Ok::<_, SilentError>("ok") });

    Server::new()
        .bind("127.0.0.1:8080".parse().unwrap())
        .serve(route)
        .await;
}
