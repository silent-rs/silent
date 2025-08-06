use super::Route;
use crate::MiddleWareHandler;
use crate::Request;
use crate::core::path_param::PathParam;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) enum RouteMatched {
    Matched(Route),
    Unmatched,
}

pub(crate) trait Match {
    fn handler_match(&self, req: &mut Request, path: &str) -> RouteMatched;

    /// 新的方法：匹配路由并收集路径上的中间件
    fn handler_match_collect_middlewares(
        &self,
        req: &mut Request,
        path: &str,
    ) -> (RouteMatched, Vec<Vec<Arc<dyn MiddleWareHandler>>>) {
        (self.handler_match(req, path), vec![])
    }
}

pub(crate) trait RouteMatch: Match {
    fn get_path(&self) -> &str;
    /// 最终匹配
    fn last_matched(&self, req: &mut Request, last_url: &str) -> RouteMatched;
    /// 最终匹配并收集中间件
    fn last_matched_collect_middlewares(
        &self,
        req: &mut Request,
        last_url: &str,
    ) -> (RouteMatched, Vec<Vec<Arc<dyn MiddleWareHandler>>>);
    fn path_split(path: &str) -> (&str, &str) {
        let mut iter = path.splitn(2, '/');
        let local_url = iter.next().unwrap_or("");
        let last_url = iter.next().unwrap_or("");
        (local_url, last_url)
    }
}

enum SpecialPath {
    String(String),
    Int(String),
    I64(String),
    I32(String),
    U64(String),
    U32(String),
    UUid(String),
    Path(String),
    FullPath(String),
}

impl From<&str> for SpecialPath {
    fn from(value: &str) -> Self {
        // 去除首尾的尖括号
        let value = &value[1..value.len() - 1];
        let mut type_str = value.splitn(2, ':');
        let key = type_str.next().unwrap_or("");
        let path_type = type_str.next().unwrap_or("");
        match path_type {
            "**" => SpecialPath::FullPath(key.to_string()),
            "*" => SpecialPath::Path(key.to_string()),
            "full_path" => SpecialPath::FullPath(key.to_string()),
            "path" => SpecialPath::Path(key.to_string()),
            "str" => SpecialPath::String(key.to_string()),
            "int" => SpecialPath::Int(key.to_string()),
            "i64" => SpecialPath::I64(key.to_string()),
            "i32" => SpecialPath::I32(key.to_string()),
            "u64" => SpecialPath::U64(key.to_string()),
            "u32" => SpecialPath::U32(key.to_string()),
            "uuid" => SpecialPath::UUid(key.to_string()),
            _ => SpecialPath::String(key.to_string()),
        }
    }
}

impl Match for Route {
    fn handler_match(&self, req: &mut Request, path: &str) -> RouteMatched {
        // 统一的路由匹配逻辑
        // 空路径的路由（包括根路由）：特殊处理
        if self.path.is_empty() {
            println!("🔍 handler_match - 空路径路由，输入路径: '{}'", path);
            let mut path = path;
            if path.starts_with('/') {
                path = &path[1..];
            }
            println!("🔍 handler_match - 处理后路径: '{}'", path);
            return self.last_matched(req, path);
        }
        
        // 普通路由的匹配逻辑
        let (local_url, last_url) = Self::path_split(path);
        if !self.special_match {
            if self.path == local_url {
                self.last_matched(req, last_url)
            } else {
                RouteMatched::Unmatched
            }
        } else {
            match self.get_path().into() {
                SpecialPath::String(key) => match self.last_matched(req, last_url) {
                    RouteMatched::Matched(route) => {
                        req.set_path_params(key, local_url.to_string().into());
                        RouteMatched::Matched(route)
                    }
                    RouteMatched::Unmatched => RouteMatched::Unmatched,
                },
                SpecialPath::Int(key) => match local_url.parse::<i32>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched(req, last_url)
                    }
                    Err(_) => RouteMatched::Unmatched,
                },
                SpecialPath::I64(key) => match local_url.parse::<i64>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched(req, last_url)
                    }
                    Err(_) => RouteMatched::Unmatched,
                },
                SpecialPath::I32(key) => match local_url.parse::<i32>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched(req, last_url)
                    }
                    Err(_) => RouteMatched::Unmatched,
                },
                SpecialPath::U64(key) => match local_url.parse::<u64>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched(req, last_url)
                    }
                    Err(_) => RouteMatched::Unmatched,
                },
                SpecialPath::U32(key) => match local_url.parse::<u32>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched(req, last_url)
                    }
                    Err(_) => RouteMatched::Unmatched,
                },
                SpecialPath::UUid(key) => match local_url.parse::<uuid::Uuid>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched(req, last_url)
                    }
                    Err(_) => RouteMatched::Unmatched,
                },
                SpecialPath::Path(key) => {
                    req.set_path_params(key, PathParam::Path(local_url.to_string()));
                    self.last_matched(req, last_url)
                }
                SpecialPath::FullPath(key) => {
                    req.set_path_params(key, PathParam::Path(path.to_string()));
                    match self.last_matched(req, last_url) {
                        RouteMatched::Matched(route) => RouteMatched::Matched(route),
                        RouteMatched::Unmatched => match self.handler.is_empty() {
                            true => RouteMatched::Unmatched,
                            false => RouteMatched::Matched(self.clone()),
                        },
                    }
                }
            }
        }
    }

    fn handler_match_collect_middlewares(
        &self,
        req: &mut Request,
        path: &str,
    ) -> (RouteMatched, Vec<Vec<Arc<dyn MiddleWareHandler>>>) {
        // 统一的路由匹配逻辑
        // 空路径的路由（包括根路由）：特殊处理
        if self.path.is_empty() {
            println!("🔍 handler_match_collect_middlewares - 空路径路由，输入路径: '{}'", path);
            let mut path = path;
            if path.starts_with('/') {
                path = &path[1..];
            }
            println!("🔍 handler_match_collect_middlewares - 处理后路径: '{}'", path);
            // 对于空路径路由，如果输入路径不是空，直接进行子路由匹配
            if !path.is_empty() {
                return self.last_matched_collect_middlewares(req, path);
            }
            // 如果输入路径是空，检查当前路由是否有处理器
            return self.last_matched_collect_middlewares(req, path);
        }
        
        // 普通路由的匹配逻辑
        let (local_url, last_url) = Self::path_split(path);
        println!("🔍 handler_match_collect_middlewares - 普通路由匹配，当前路径: '{}', 本地URL: '{}', 剩余URL: '{}'", self.path, local_url, last_url);
        
        // 普通路由的匹配逻辑
        let (local_url, last_url) = Self::path_split(path);
        if !self.special_match {
            if self.path == local_url {
                self.last_matched_collect_middlewares(req, last_url)
            } else {
                (RouteMatched::Unmatched, vec![])
            }
        } else {
            match self.get_path().into() {
                SpecialPath::String(key) => {
                    let (matched, middleware_layers) =
                        self.last_matched_collect_middlewares(req, last_url);
                    match matched {
                        RouteMatched::Matched(route) => {
                            req.set_path_params(key, local_url.to_string().into());
                            (RouteMatched::Matched(route), middleware_layers)
                        }
                        RouteMatched::Unmatched => (RouteMatched::Unmatched, vec![]),
                    }
                }
                SpecialPath::Int(key) => match local_url.parse::<i32>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched_collect_middlewares(req, last_url)
                    }
                    Err(_) => (RouteMatched::Unmatched, vec![]),
                },
                SpecialPath::I64(key) => match local_url.parse::<i64>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched_collect_middlewares(req, last_url)
                    }
                    Err(_) => (RouteMatched::Unmatched, vec![]),
                },
                SpecialPath::I32(key) => match local_url.parse::<i32>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched_collect_middlewares(req, last_url)
                    }
                    Err(_) => (RouteMatched::Unmatched, vec![]),
                },
                SpecialPath::U64(key) => match local_url.parse::<u64>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched_collect_middlewares(req, last_url)
                    }
                    Err(_) => (RouteMatched::Unmatched, vec![]),
                },
                SpecialPath::U32(key) => match local_url.parse::<u32>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched_collect_middlewares(req, last_url)
                    }
                    Err(_) => (RouteMatched::Unmatched, vec![]),
                },
                SpecialPath::UUid(key) => match local_url.parse::<uuid::Uuid>() {
                    Ok(value) => {
                        req.set_path_params(key, value.into());
                        self.last_matched_collect_middlewares(req, last_url)
                    }
                    Err(_) => (RouteMatched::Unmatched, vec![]),
                },
                SpecialPath::Path(key) => {
                    req.set_path_params(key, PathParam::Path(local_url.to_string()));
                    self.last_matched_collect_middlewares(req, last_url)
                }
                SpecialPath::FullPath(key) => {
                    req.set_path_params(key, PathParam::Path(path.to_string()));
                    let (matched, middleware_layers) =
                        self.last_matched_collect_middlewares(req, last_url);
                    match matched {
                        RouteMatched::Matched(route) => {
                            (RouteMatched::Matched(route), middleware_layers)
                        }
                        RouteMatched::Unmatched => match self.handler.is_empty() {
                            true => (RouteMatched::Unmatched, vec![]),
                            false => {
                                let mut layers = vec![];
                                if !self.middlewares.is_empty() {
                                    layers.push(self.middlewares.clone());
                                }
                                (RouteMatched::Matched(self.clone()), layers)
                            }
                        },
                    }
                }
            }
        }
    }
}

impl RouteMatch for Route {
    fn get_path(&self) -> &str {
        self.path.as_str()
    }

    fn last_matched(&self, req: &mut Request, last_url: &str) -> RouteMatched {
        if last_url.is_empty() {
            println!("🔍 last_matched - 路径匹配完成，当前路由路径: '{}', 处理器数量: {}", self.path, self.handler.len());
            // 如果当前路由有对应方法的handler，返回匹配
            if self.handler.contains_key(req.method()) {
                println!("🔍 last_matched - 找到匹配路由，路径: '{}', 方法: {:?}", self.path, req.method());
                let mut cloned_route = self.clone();
                // 确保克隆的路由包含正确的configs信息
                if cloned_route.configs.is_none() && self.configs.is_some() {
                    cloned_route.configs = self.configs.clone();
                    println!("🔍 last_matched - 已复制configs到克隆路由");
                }
                println!("🔍 last_matched - 克隆后路由，路径: '{}', 处理器数量: {}, 有configs: {}", 
                        cloned_route.path, cloned_route.handler.len(), cloned_route.configs.is_some());
                return RouteMatched::Matched(cloned_route);
            } else {
                println!("🔍 last_matched - 路径匹配但方法不匹配，路径: '{}', 方法: {:?}, 可用方法: {:?}", 
                        self.path, req.method(), self.handler.keys().collect::<Vec<_>>());
                // 如果路径匹配但没有对应方法的handler，返回未匹配（这样会返回404而不是405）
                return RouteMatched::Unmatched;
            }
        } else {
            println!("🔍 last_matched - 检查 {} 个子路由", self.children.len());
            for (i, route) in self.children.iter().enumerate() {
                println!("🔍 last_matched - 检查子路由 {}: 路径='{}', 处理器数量={}", i, route.path, route.handler.len());
                if let RouteMatched::Matched(route) = route.handler_match(req, last_url) {
                    println!("🔍 last_matched - 子路由 {} 匹配成功", i);
                    return RouteMatched::Matched(route);
                }
            }
        }
        RouteMatched::Unmatched
    }

    fn last_matched_collect_middlewares(
        &self,
        req: &mut Request,
        last_url: &str,
    ) -> (RouteMatched, Vec<Vec<Arc<dyn MiddleWareHandler>>>) {
        tracing::debug!("last_matched_collect_middlewares: path='{}', last_url='{}', has_handler={}", 
                       self.path, last_url, !self.handler.is_empty());
        // 如果是最终路由（URL已经完全匹配），检查是否有对应方法的handler
        if last_url.is_empty() {
            // 对于空路径路由，如果有子路由，优先检查子路由
            if !self.children.is_empty() {
                println!("🔍 last_matched_collect_middlewares - 空路径路由有子路由，优先检查子路由");
                for (i, route) in self.children.iter().enumerate() {
                    println!("🔍 last_matched_collect_middlewares - 检查子路由 {}: 路径='{}', 处理器数量={}", i, route.path, route.handler.len());
                    let (matched, mut middleware_layers) =
                        route.handler_match_collect_middlewares(req, last_url);
                    if let RouteMatched::Matched(matched_route) = matched {
                        println!("🔍 last_matched_collect_middlewares - 子路由 {} 匹配成功", i);
                        // 如果当前层有中间件，添加到层级的前面
                        if !self.middlewares.is_empty() {
                            middleware_layers.insert(0, self.middlewares.clone());
                        }
                        return (RouteMatched::Matched(matched_route), middleware_layers);
                    }
                }
                println!("🔍 last_matched_collect_middlewares - 所有子路由都匹配失败");
            }
            
            let mut middleware_layers = vec![];
            if !self.middlewares.is_empty() {
                middleware_layers.push(self.middlewares.clone());
            }
            
            // 如果当前路由有对应方法的handler，返回匹配
            if self.handler.contains_key(req.method()) {
                println!("🔍 last_matched_collect_middlewares - 路径完全匹配，当前路由有处理器，返回匹配");
                let mut cloned_route = self.clone();
                // 确保克隆的路由包含正确的configs信息
                if cloned_route.configs.is_none() && self.configs.is_some() {
                    cloned_route.configs = self.configs.clone();
                }
                return (RouteMatched::Matched(cloned_route), middleware_layers);
            } else {
                println!("🔍 last_matched_collect_middlewares - 路径完全匹配，但当前路由没有处理器，返回未匹配");
                // 如果路径匹配但没有对应方法的handler，返回未匹配（这样会返回404而不是405）
                return (RouteMatched::Unmatched, vec![]);
            }
        } else {
            // 对于空路径路由，优先匹配子路由，而不是检查当前路由的处理器
            if self.path.is_empty() {
                println!("🔍 last_matched_collect_middlewares - 空路径路由，优先匹配子路由，剩余URL: '{}'", last_url);
                // 继续向子路由匹配
                for (i, route) in self.children.iter().enumerate() {
                    println!("🔍 last_matched_collect_middlewares - 检查子路由 {}: 路径='{}', 处理器数量={}", i, route.path, route.handler.len());
                    let (matched, mut middleware_layers) =
                        route.handler_match_collect_middlewares(req, last_url);
                    if let RouteMatched::Matched(matched_route) = matched {
                        println!("🔍 last_matched_collect_middlewares - 子路由 {} 匹配成功", i);
                        // 如果当前层有中间件，添加到层级的前面
                        if !self.middlewares.is_empty() {
                            middleware_layers.insert(0, self.middlewares.clone());
                        }
                        return (RouteMatched::Matched(matched_route), middleware_layers);
                    }
                }
                println!("🔍 last_matched_collect_middlewares - 所有子路由都匹配失败");
                return (RouteMatched::Unmatched, vec![]);
            }
            
            // 如果剩余URL不是空，优先匹配子路由
            if !self.children.is_empty() {
                println!("🔍 last_matched_collect_middlewares - 检查 {} 个子路由，剩余URL: '{}'", self.children.len(), last_url);
                // 继续向子路由匹配
                for (i, route) in self.children.iter().enumerate() {
                    println!("🔍 last_matched_collect_middlewares - 检查子路由 {}: 路径='{}', 处理器数量={}", i, route.path, route.handler.len());
                    let (matched, mut middleware_layers) =
                        route.handler_match_collect_middlewares(req, last_url);
                    if let RouteMatched::Matched(matched_route) = matched {
                        println!("🔍 last_matched_collect_middlewares - 子路由 {} 匹配成功", i);
                        // 如果当前层有中间件，添加到层级的前面
                        if !self.middlewares.is_empty() {
                            middleware_layers.insert(0, self.middlewares.clone());
                        }
                        return (RouteMatched::Matched(matched_route), middleware_layers);
                    }
                }
                println!("🔍 last_matched_collect_middlewares - 所有子路由都匹配失败");
            }
            
            // 如果子路由都匹配失败，再检查当前路由是否有对应方法的handler
            if self.handler.contains_key(req.method()) {
                println!("🔍 last_matched_collect_middlewares - 当前路由有对应方法的处理器，返回匹配");
                let mut middleware_layers = vec![];
                if !self.middlewares.is_empty() {
                    middleware_layers.push(self.middlewares.clone());
                }
                let mut cloned_route = self.clone();
                // 确保克隆的路由包含正确的configs信息
                if cloned_route.configs.is_none() && self.configs.is_some() {
                    cloned_route.configs = self.configs.clone();
                }
                return (RouteMatched::Matched(cloned_route), middleware_layers);
            }
            
            println!("🔍 last_matched_collect_middlewares - 检查 {} 个子路由，剩余URL: '{}'", self.children.len(), last_url);
            // 继续向子路由匹配
            for (i, route) in self.children.iter().enumerate() {
                println!("🔍 last_matched_collect_middlewares - 检查子路由 {}: 路径='{}', 处理器数量={}", i, route.path, route.handler.len());
                let (matched, mut middleware_layers) =
                    route.handler_match_collect_middlewares(req, last_url);
                if let RouteMatched::Matched(matched_route) = matched {
                    println!("🔍 last_matched_collect_middlewares - 子路由 {} 匹配成功", i);
                    // 如果当前层有中间件，添加到层级的前面
                    if !self.middlewares.is_empty() {
                        middleware_layers.insert(0, self.middlewares.clone());
                    }
                    return (RouteMatched::Matched(matched_route), middleware_layers);
                }
            }
            println!("🔍 last_matched_collect_middlewares - 所有子路由都匹配失败");
        }

        (RouteMatched::Unmatched, vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::HandlerAppend;
    use crate::{Handler, SilentError, Method};
    use bytes::Bytes;
    use http_body_util::BodyExt;

    async fn hello(_: Request) -> Result<String, SilentError> {
        Ok("hello".to_string())
    }

    async fn world<'a>(_: Request) -> Result<&'a str, SilentError> {
        Ok("world")
    }

    fn get_matched(routes: &Route, req: Request) -> bool {
        let (mut req, path) = req.split_url();
        match routes.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(_) => true,
            RouteMatched::Unmatched => false,
        }
    }

    #[test]
    fn route_match_test() {
        let route = Route::new("hello").get(hello);
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/hello".parse().unwrap();
        assert!(get_matched(&routes, req));
    }

    #[test]
    fn multi_route_match_test() {
        let route = Route::new("hello/world").get(hello);
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/hello/world".parse().unwrap();
        assert!(get_matched(&routes, req));
    }

    #[test]
    fn multi_route_match_test_2() {
        let route = Route::new("")
            .get(hello)
            .append(Route::new("world").get(hello));
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/world".parse().unwrap();
        assert!(get_matched(&routes, req));
    }

    #[test]
    fn multi_route_match_test_3() {
        let route = Route::new("")
            .get(hello)
            .append(Route::new("<id:i64>").get(hello));
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/12345678909876543".parse().unwrap();
        let (mut req, path) = req.split_url();
        let matched = match routes.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(_) => {
                assert_eq!(
                    req.get_path_params::<i64>("id").unwrap(),
                    12345678909876543i64
                );
                true
            }
            RouteMatched::Unmatched => false,
        };
        assert!(matched)
    }

    #[test]
    fn special_route_match_test_2() {
        let route = Route::new("<path:**>")
            .get(hello)
            .append(Route::new("world").get(hello));
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/hello/world".parse().unwrap();
        let (mut req, path) = req.split_url();
        let matched = match routes.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(_) => {
                assert_eq!(
                    req.get_path_params::<String>("path").unwrap(),
                    "hello/world".to_string()
                );
                true
            }
            RouteMatched::Unmatched => false,
        };
        assert!(matched)
    }

    #[tokio::test]
    async fn special_route_match_test_3() {
        let route = Route::new("<path:**>")
            .get(hello)
            .append(Route::new("world").get(world));
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
        *req.uri_mut() = "/hello/world".parse().unwrap();
        assert_eq!(
            routes
                .call(req)
                .await
                .unwrap()
                .body
                .frame()
                .await
                .unwrap()
                .unwrap()
                .data_ref()
                .unwrap(),
            &Bytes::from("world")
        );
    }

    #[tokio::test]
    async fn special_route_match_test_4() {
        let route = Route::new("<path:**>")
            .get(hello)
            .append(Route::new("world").get(world));
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
        *req.uri_mut() = "/hello/world1".parse().unwrap();
        assert_eq!(
            routes
                .call(req)
                .await
                .unwrap()
                .body
                .frame()
                .await
                .unwrap()
                .unwrap()
                .data_ref()
                .unwrap(),
            &Bytes::from("hello")
        );
    }

    // 边界情况测试
    #[test]
    fn empty_path_edge_case_test() {
        // 测试空路径路由的匹配
        let route = Route::new("").get(hello);
        let mut routes = Route::new_root();
        routes.push(route);
        
        // 测试根路径
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();
        assert!(get_matched(&routes, req));
        
        // 测试空路径 - 空路径无法解析为URI，应该跳过这个测试
        // let mut req = Request::empty();
        // *req.uri_mut() = "".parse().unwrap();
        // assert!(get_matched(&routes, req));
    }

    #[test]
    fn nested_empty_path_test() {
        // 测试嵌套的空路径路由
        let route = Route::new("")
            .get(hello)
            .append(Route::new("").get(world));
        let mut routes = Route::new_root();
        routes.push(route);
        
        // 测试根路径应该匹配第一个处理器
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();
        assert!(get_matched(&routes, req));
    }

    #[test]
    fn path_conflict_test() {
        // 测试路径冲突情况
        let route = Route::new("")
            .append(Route::new("api").get(hello))
            .append(Route::new("api/v1").get(world));
        let mut routes = Route::new_root();
        routes.push(route);
        
        // 测试 /api 应该匹配第一个
        let mut req = Request::empty();
        *req.uri_mut() = "/api".parse().unwrap();
        assert!(get_matched(&routes, req));
        
        // 测试 /api/v1 应该匹配第二个
        let mut req = Request::empty();
        *req.uri_mut() = "/api/v1".parse().unwrap();
        assert!(get_matched(&routes, req));
    }

    #[test]
    fn trailing_slash_test() {
        // 测试尾随斜杠的处理
        let route = Route::new("test").get(hello);
        let mut routes = Route::new_root();
        routes.push(route);
        
        // 测试 /test 应该匹配
        let mut req = Request::empty();
        *req.uri_mut() = "/test".parse().unwrap();
        assert!(get_matched(&routes, req));
        
        // 测试 /test/ 实际上会匹配到 /test 路由（当前实现的行为）
        // 这是因为 path_split("test/") 返回 ("test", "")，然后匹配成功
        let mut req = Request::empty();
        *req.uri_mut() = "/test/".parse().unwrap();
        assert!(get_matched(&routes, req));
        
        // 测试 /test/extra 不应该匹配
        let mut req = Request::empty();
        *req.uri_mut() = "/test/extra".parse().unwrap();
        assert!(!get_matched(&routes, req));
    }

    #[test]
    fn special_path_parameter_edge_cases() {
        // 测试特殊路径参数的边界情况
        let route = Route::new("")
            .append(Route::new("user/<id:i64>").get(hello))
            .append(Route::new("post/<slug>").get(world));
        let mut routes = Route::new_root();
        routes.push(route);
        
        // 测试有效的数字参数
        let mut req = Request::empty();
        *req.uri_mut() = "/user/123".parse().unwrap();
        let (mut req, path) = req.split_url();
        let matched = match routes.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(_) => {
                assert_eq!(req.get_path_params::<i64>("id").unwrap(), 123);
                true
            }
            RouteMatched::Unmatched => false,
        };
        assert!(matched);
        
        // 测试无效的数字参数应该不匹配
        let mut req = Request::empty();
        *req.uri_mut() = "/api/user/abc".parse().unwrap();
        let (mut req, path) = req.split_url();
        assert!(!matches!(routes.handler_match(&mut req, path.as_str()), RouteMatched::Matched(_)));
        
        // 测试字符串参数
        let mut req = Request::empty();
        *req.uri_mut() = "/post/hello-world".parse().unwrap();
        let (mut req, path) = req.split_url();
        let matched = match routes.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(_) => {
                assert_eq!(req.get_path_params::<String>("slug").unwrap(), "hello-world");
                true
            }
            RouteMatched::Unmatched => false,
        };
        assert!(matched);
    }

    #[test]
    fn root_route_matching_test() {
        // 测试根路由匹配问题
        println!("=== 测试根路由匹配 ===");
        
        // 测试1: 根路由（没有处理器）
        let mut root_route = Route::new_root();
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();
        
        let (mut req, path) = req.split_url();
        println!("请求路径: '{}'", path);
        
        match root_route.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(route) => {
                println!("✅ 匹配成功，路由路径: '{}'", route.path);
                println!("路由有处理器: {}", !route.handler.is_empty());
            }
            RouteMatched::Unmatched => {
                println!("❌ 匹配失败");
            }
        }
        
        // 测试2: 空路径路由（有处理器）
        let app = Route::new("").get(hello);
        println!("空路径路由创建完成，路径: '{}', 创建路径: '{}'", app.path, app.create_path);
        println!("空路径路由有处理器: {}", !app.handler.is_empty());
        println!("空路径路由有GET处理器: {}", app.handler.contains_key(&Method::GET));
        
        let mut root_route = Route::new_root();
        root_route.push(app);
        
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();
        
        let (mut req, path) = req.split_url();
        println!("请求路径: '{}'", path);
        
        match root_route.handler_match(&mut req, path.as_str()) {
            RouteMatched::Matched(route) => {
                println!("✅ 匹配成功，路由路径: '{}'", route.path);
                println!("路由有处理器: {}", !route.handler.is_empty());
            }
            RouteMatched::Unmatched => {
                println!("❌ 匹配失败");
            }
        }
    }
}
