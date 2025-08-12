use silent::prelude::{Request, Route};
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
extern "C" fn get_route() -> Route {
    Route::new("hello").get(|_req: Request| async { Ok("hello world") })
}
