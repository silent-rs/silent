use async_trait::async_trait;
use tonic::{Request as TonicRequest, Response, Status};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};
use silent::prelude::{Level, Route, Server, info, logger};
use silent::{GrpcRegister, Request};

mod client;

pub mod hello_world {
    tonic::include_proto!("hello_world"); // The string specified here must match the proto package name
}

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: TonicRequest<HelloRequest>, // Accept request of type HelloRequest
    ) -> Result<Response<HelloReply>, Status> {
        // Return an instance of type HelloReply
        info!("Got a request: {:?}", request);

        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name), // We must use .into_inner() as the fields of gRPC requests and responses are private
        };

        Ok(Response::new(reply)) // Send back our formatted greeting
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let greeter = MyGreeter::default();
    logger::fmt().with_max_level(Level::INFO).init();
    let greeter_server = GreeterServer::new(greeter);
    // let grpc = TonicServer::builder()
    //     // Wrap all services in the middleware stack
    //     .add_service(greeter_server)
    //     .into_router();
    let route = Route::new("")
        .get(|_req: Request| async { Ok("hello world") })
        .append(greeter_server.service());
    info!("route: \n{:?}", route);
    Server::new()
        .bind("0.0.0.0:50051".parse().unwrap()).expect("Failed to bind to address")
        .serve(route)
        .await;
    Ok(())
}
