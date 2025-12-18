use crate::middleware::MiddleWareHandler;
use crate::route::route_tree::SpecialSeg;
use crate::route::route_tree::parse_special_seg;
use crate::route::{Route, RouteTree};
use smallvec::SmallVec;
use std::collections::HashMap;
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

        let mut static_children = HashMap::new();
        let mut dynamic_children = SmallVec::<[usize; 4]>::new();
        for (idx, child) in children.iter().enumerate() {
            if let Some(key) = child.segment.as_static_key() {
                static_children.insert(key.into(), idx);
            } else {
                dynamic_children.push(idx);
            }
        }

        fn dynamic_rank(seg: &SpecialSeg) -> u8 {
            match seg {
                SpecialSeg::Int { .. }
                | SpecialSeg::I64 { .. }
                | SpecialSeg::I32 { .. }
                | SpecialSeg::U64 { .. }
                | SpecialSeg::U32 { .. }
                | SpecialSeg::Uuid { .. } => 0,
                // `<key>` 与 `<key:path>` 都是“任意单段”，保持同一优先级。
                SpecialSeg::String { .. } | SpecialSeg::Path { .. } => 1,
                // `<key:**>` 最宽泛，优先级最低。
                SpecialSeg::FullPath { .. } => 2,
                // Root/Static 不会进入 dynamic_children。
                SpecialSeg::Root | SpecialSeg::Static(_) => 3,
            }
        }

        dynamic_children.sort_by_key(|&idx| (dynamic_rank(&children[idx].segment), idx));

        RouteTree {
            children,
            handler,
            middlewares: current_middlewares,
            static_children,
            dynamic_children,
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
