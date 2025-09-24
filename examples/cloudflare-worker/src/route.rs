use silent::{
    Request,
    prelude::{Route, WorkRoute},
};

pub fn get_route() -> WorkRoute {
    // 示例路由：GET /hello
    let route = Route::new_root().append(Route::new("hello").get(|_r: Request| async move {
        Ok("hello from Cloudflare Worker via Silent".to_string())
    }));
    WorkRoute::new(route)
}
