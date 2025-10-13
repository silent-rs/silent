#![cfg(feature = "quic")]

mod connection;
mod core;
mod echo;
mod listener;
mod middleware;
mod service;

pub use listener::QuicEndpointListener;

use anyhow::Result;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use crate::prelude::Route;
use crate::service::CertificateStore;
use crate::service::Server;
use crate::service::listener::Listener;

pub async fn run_server(
    routes: Route,
    bind_addr: SocketAddr,
    store: &CertificateStore,
) -> Result<()> {
    let routes = Arc::new(routes.hook(middleware::AltSvcMiddleware::new(bind_addr.port())));
    let http_listener = Listener::from(TcpListener::bind(bind_addr.clone())?).tls_with_cert(store);

    let service = service::HybridService::new(routes, bind_addr.port());

    let server = Server::new()
        .listen(http_listener)
        .listen(QuicEndpointListener::new(bind_addr, store));
    server.serve(service).await;
    Ok(())
}
