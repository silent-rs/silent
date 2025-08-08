use crate::route::{Route, RouteTree};

pub trait RouteService {
    fn route(self) -> Route;
}

impl RouteService for Route {
    fn route(self) -> Route {
        self
    }
}

impl Route {
    /// 递归将Route转换为RouteTree
    pub(crate) fn convert_to_route_tree(self) -> RouteTree {
        // 先克隆需要的数据，避免移动问题
        let children = self.children;
        let handler = self.handler;
        let middlewares = self.middlewares;
        let configs = self.configs;
        let special_match = self.special_match;
        let path = self.path;
        let has_handler = !handler.is_empty();

        // 递归处理子路由
        let children: Vec<RouteTree> = children
            .into_iter()
            .map(|child| child.convert_to_route_tree())
            .collect();

        RouteTree {
            children,
            handler,
            middlewares,
            configs,
            special_match,
            path,
            has_handler,
        }
    }
}
