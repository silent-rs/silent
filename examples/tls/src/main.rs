use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use silent::Request;
use silent::prelude::{Level, Listener, Route, Server, logger};
use std::sync::{Arc, OnceLock};
use tokio_rustls::{TlsAcceptor, rustls};

fn ensure_crypto_provider() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        // 选择 ring 作为进程级 CryptoProvider，避免 rustls 在多 provider 特性下自动判定失败。
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[tokio::main]
async fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    ensure_crypto_provider();
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
    // 浏览器 HTTP/2 一般依赖 HTTPS + ALPN(h2) 协商，否则会回退到 HTTP/1.1。
    let mut config = config;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
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
