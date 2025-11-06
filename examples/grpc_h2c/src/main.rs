// use tonic::transport::server::TowerToHyperService;
use tonic::{Request as TonicRequest, Response, Status};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};
use silent::GrpcRegister;
use silent::Request;
use silent::prelude::{Level, Route, info, logger};

pub mod hello_world {
    tonic::include_proto!("hello_world");
}

#[derive(Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: TonicRequest<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        info!("Got a request from {:?}", request.remote_addr());

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let greeter = MyGreeter::default();
    logger::fmt().with_max_level(Level::INFO).init();

    let mut route = Route::new("").get(|_req: Request| async { Ok("hello world") });
    GreeterServer::new(greeter).register(&mut route);
    silent::prelude::Server::new()
        .bind("0.0.0.0:50051".parse().unwrap())
        .serve(route)
        .await;
    Ok(())
}
