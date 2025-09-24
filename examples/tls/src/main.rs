use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use silent::Request;
use silent::prelude::{Level, Route, Server, logger};
use std::sync::Arc;

fn main() {
    async_global_executor::block_on(async move {
        logger::fmt().with_max_level(Level::INFO).init();
        let route = Route::new("").get(|_req: Request| async { Ok("hello world") });
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
        let acceptor = futures_rustls::TlsAcceptor::from(Arc::new(config));
        let addr: std::net::SocketAddr = "127.0.0.1:8443".parse().unwrap();
        Server::new()
            .with_async_tls(acceptor)
            .bind(addr)
            .serve(route)
            .await;
    });
}
