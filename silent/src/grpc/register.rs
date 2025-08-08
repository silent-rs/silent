use crate::grpc::GrpcHandler;
use crate::prelude::HandlerGetter;
use crate::prelude::Route;
use http::Method;
use std::sync::Arc;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::server::NamedService;

pub trait GrpcRegister<S> {
    fn get_handler(self) -> GrpcHandler<S>;
    fn service(self) -> Route;
    fn register(self, route: &mut Route);
}

impl<S> GrpcRegister<S> for S
where
    S: Service<http::Request<Body>, Response = http::Response<Body>> + NamedService,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    fn get_handler(self) -> GrpcHandler<S> {
        GrpcHandler::new(self)
    }
    fn service(self) -> Route {
        let handler = self.get_handler();
        let path = handler.path().to_string();
        let handler = Arc::new(handler);
        Route::new(path.as_str()).append(
            Route::new("<path:**>")
                .insert_handler(Method::POST, handler.clone())
                .insert_handler(Method::GET, handler),
        )
    }

    fn register(self, route: &mut Route) {
        route.push(self.service());
    }
}
