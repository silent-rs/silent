use crate::middleware::MiddleWareHandler;
use crate::route::route_tree::parse_special_seg;
use crate::route::{Route, RouteTree};
use std::sync::Arc;

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
        let empty: Arc<[Arc<dyn MiddleWareHandler>]> = Arc::from(Vec::new());
        self.into_route_tree_with_chain(empty)
    }

    fn into_route_tree_with_chain(
        self,
        inherited_middlewares: Arc<[Arc<dyn MiddleWareHandler>]>,
    ) -> RouteTree {
        let Route {
            path,
            handler,
            children,
            middlewares,
            configs,
            ..
        } = self;

        let segment = parse_special_seg(path);
        let has_handler = !handler.is_empty();

        let parent_len = inherited_middlewares.len();

        let current_middlewares = if middlewares.is_empty() {
            inherited_middlewares.clone()
        } else {
            let mut merged = Vec::with_capacity(inherited_middlewares.len() + middlewares.len());
            merged.extend(inherited_middlewares.iter().cloned());
            merged.extend(middlewares);
            Arc::from(merged)
        };

        let children: Vec<RouteTree> = children
            .into_iter()
            .map(|child| child.into_route_tree_with_chain(current_middlewares.clone()))
            .collect();

        RouteTree {
            children,
            handler,
            middlewares: current_middlewares,
            middleware_start: parent_len,
            configs,
            segment,
            has_handler,
        }
    }

    pub fn into_route_tree(self) -> RouteTree {
        self.convert_to_route_tree()
    }
}
