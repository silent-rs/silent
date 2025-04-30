use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use silent::prelude::{HandlerAppend, Level, Listener, Route, Server, logger};
use std::sync::Arc;
use tokio_rustls::{TlsAcceptor, rustls};

#[tokio::main]
async fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").get(|_req| async { Ok("hello world") });
    println!(
        "current dir: {}",
        std::env::current_dir().unwrap().display()
    );
    let certs = CertificateDer::pem_file_iter("./examples/tls/certs/localhost+2.pem")
        .expect("failed to load certificate file")
        .collect::<Result<Vec<_>, _>>()
        .expect("failed to parse certificate file");
    let key = PrivateKeyDer::from_pem_file("./examples/tls/certs/localhost+2-key.pem")
        .expect("failed to load private key file");

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener: Listener = tokio::net::TcpListener::bind("127.0.0.1:8443")
        .await
        .expect("failed to bind")
        .into();
    Server::new()
        .listen(listener.tls(acceptor))
        .serve(route)
        .await;
}
