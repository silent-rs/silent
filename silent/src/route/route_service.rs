use crate::route::Route;

pub trait RouteService {
    fn route(self) -> Route;
}

impl RouteService for Route {
    fn route(self) -> Route {
        // 如果已经是服务入口点（有配置），直接返回
        if self.configs.is_some() {
            self
        } else {
            // 否则创建新的根路由并添加当前路由为子路由
            let mut root = Route::new_root();
            root.push(self);
            root
        }
    }
}
