use crate::conn::SilentConnection;
use crate::route::{Route, Routes};
use bytes::Bytes;
use http_body_util::Full;
use hyper::service::Service;
use hyper::{body::Incoming as IncomingBody, Request, Response};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

pub struct Server {
    routes: Arc<RwLock<Routes>>,
    addr: SocketAddr,
    conn: Arc<SilentConnection>,
    rt: Runtime,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(RwLock::new(Routes::new())),
            addr: ([127, 0, 0, 1], 8000).into(),
            conn: Arc::new(SilentConnection::default()),
            rt: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        }
    }

    pub fn bind(&mut self, addr: SocketAddr) -> &mut Self {
        self.addr = addr;
        self
    }

    pub fn bind_route(&mut self, route: Route) -> &mut Self {
        self.rt.block_on(self.routes.write()).add(route);
        self
    }

    pub async fn serve(&self) {
        let Self { conn, routes, .. } = self;
        println!("Listening on http://{}", self.addr);
        let listener = TcpListener::bind(self.addr).await.unwrap();
        // let conn = Arc::new(conn.clone());
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("Accepting from: {}", stream.peer_addr().unwrap());
                    println!("{}", addr);
                    let routes = routes.read().await.clone();
                    let conn = conn.clone();
                    tokio::task::spawn(async move {
                        if let Err(err) =
                            conn.http1.serve_connection(stream, Serve { routes }).await
                        {
                            println!("Failed to serve connection: {:?}", err);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!(error = ?e, "accept connection failed");
                }
            }
        }
    }

    pub fn run(&self) {
        self.rt.block_on(self.serve());
    }
}

struct Serve {
    routes: Routes,
}

impl Service<Request<IncomingBody>> for Serve {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&mut self, req: Request<IncomingBody>) -> Self::Future {
        fn mk_response(s: String) -> Result<Response<Full<Bytes>>, hyper::Error> {
            Ok(Response::builder().body(Full::new(Bytes::from(s))).unwrap())
        }

        println!("req: {:?}", self.routes);

        let res = match req.uri().path() {
            "/" => mk_response(format!("home! counter = {:?}", 1)),
            "/posts" => mk_response(format!("posts, of course! counter = {:?}", 1)),
            "/authors" => mk_response(format!("authors extraordinare! counter = {:?}", 1)),
            // Return the 404 Not Found for other routes, and don't increment counter.
            _ => return Box::pin(async { mk_response("oh no! not found".into()) }),
        };

        Box::pin(async { res })
    }
}
