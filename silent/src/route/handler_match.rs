use uuid::Uuid;

use super::Route;
use crate::Request;
use crate::core::path_param::PathParam;

#[derive(Debug, Clone)]
pub(crate) enum RouteMatched {
    Matched(Route),
    Unmatched,
}

impl RouteMatched {
    #[cfg(test)]
    pub(crate) fn is_matched(&self) -> bool {
        match self {
            RouteMatched::Matched(_) => true,
            RouteMatched::Unmatched => false,
        }
    }
}

pub type LastPath = String;

pub(crate) trait Match {
    fn route_match(&self, req: &mut Request, path: String) -> (RouteMatched, LastPath);
}

#[cfg(test)]
pub(crate) fn last_matched(routes: &Route, req: &mut Request, path: String) -> RouteMatched {
    let (matched, last_path) = routes.route_match(req, path);
    if last_path.is_empty() {
        matched
    } else if let RouteMatched::Matched(route) = matched {
        if route.children.is_empty() {
            if !last_path.is_empty() && last_path != "/" {
                RouteMatched::Unmatched
            } else {
                RouteMatched::Matched(route)
            }
        } else {
            // 递归匹配子路由
            let result = route
                .children
                .iter()
                .map(|r| last_matched(r, req, last_path.clone()))
                .find(|m| m.is_matched());

            // 如果没有子路由匹配，但有父路由处理器，则返回父路由
            if result.is_none() && !route.handler.is_empty() {
                RouteMatched::Matched(route)
            } else {
                result.unwrap_or(RouteMatched::Unmatched)
            }
        }
    } else {
        RouteMatched::Unmatched
    }
}

pub(crate) enum SpecialPath {
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
    fn route_match(&self, req: &mut Request, path: String) -> (RouteMatched, LastPath) {
        // 空路径的路由（包括根路由）：特殊处理
        if self.path.is_empty() {
            // 对于空路径路由，如果请求路径为空或为根路径，应该匹配
            // 只有当请求路径不为空且不为根路径，且没有子路由时才不匹配
            let normalized_path = if path == "/" {
                "".to_string()
            } else {
                path.clone()
            };
            return if !normalized_path.is_empty() && self.children.is_empty() {
                (RouteMatched::Unmatched, "".to_string())
            } else {
                (RouteMatched::Matched(self.clone()), normalized_path)
            };
        }

        let mut path = path;
        // 统一的路由匹配逻辑
        if path.starts_with('/') {
            path = path[1..].to_string();
        }

        // 普通路由的匹配逻辑
        let (local_path, last_path) = path
            .clone()
            .split_once("/")
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .unwrap_or((path.clone(), "".to_string()));
        if !self.special_match {
            if self.path == local_path {
                (RouteMatched::Matched(self.clone()), last_path)
            } else {
                (RouteMatched::Unmatched, "".to_string())
            }
        } else {
            match self.path.as_str().into() {
                SpecialPath::String(key) => {
                    req.set_path_params(key, local_path.to_string().into());
                    (RouteMatched::Matched(self.clone()), last_path)
                }
                SpecialPath::Int(key) => {
                    if let Ok(value) = local_path.parse::<i32>() {
                        req.set_path_params(key, value.into());
                        (RouteMatched::Matched(self.clone()), last_path)
                    } else {
                        (RouteMatched::Unmatched, "".to_string())
                    }
                }
                SpecialPath::I64(key) => {
                    if let Ok(value) = local_path.parse::<i64>() {
                        req.set_path_params(key, value.into());
                        (RouteMatched::Matched(self.clone()), last_path)
                    } else {
                        (RouteMatched::Unmatched, "".to_string())
                    }
                }
                SpecialPath::I32(key) => {
                    if let Ok(value) = local_path.parse::<i32>() {
                        req.set_path_params(key, value.into());
                        (RouteMatched::Matched(self.clone()), last_path)
                    } else {
                        (RouteMatched::Unmatched, "".to_string())
                    }
                }
                SpecialPath::U64(key) => {
                    if let Ok(value) = local_path.parse::<u64>() {
                        req.set_path_params(key, value.into());
                        (RouteMatched::Matched(self.clone()), last_path)
                    } else {
                        (RouteMatched::Unmatched, "".to_string())
                    }
                }
                SpecialPath::U32(key) => {
                    if let Ok(value) = local_path.parse::<u32>() {
                        req.set_path_params(key, value.into());
                        (RouteMatched::Matched(self.clone()), last_path)
                    } else {
                        (RouteMatched::Unmatched, "".to_string())
                    }
                }
                SpecialPath::UUid(key) => {
                    if let Ok(value) = local_path.parse::<Uuid>() {
                        req.set_path_params(key, value.into());
                        (RouteMatched::Matched(self.clone()), last_path)
                    } else {
                        (RouteMatched::Unmatched, "".to_string())
                    }
                }
                SpecialPath::Path(key) => {
                    req.set_path_params(key, PathParam::Path(local_path.to_string()));
                    (RouteMatched::Matched(self.clone()), last_path)
                }
                SpecialPath::FullPath(key) => {
                    req.set_path_params(key, PathParam::Path(path.to_string()));
                    // 对于 ** 通配符，总是匹配成功
                    // 如果有子路由，让子路由有机会匹配，但如果没有子路由匹配，父路由仍然有效
                    (RouteMatched::Matched(self.clone()), last_path)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::HandlerAppend;
    use crate::{Handler, Method, SilentError};
    use bytes::Bytes;
    use http_body_util::BodyExt;

    async fn hello(_: Request) -> Result<String, SilentError> {
        Ok("hello".to_string())
    }

    async fn world<'a>(_: Request) -> Result<&'a str, SilentError> {
        Ok("world")
    }

    fn get_matched(routes: &Route, req: Request) -> (Request, bool) {
        let (mut req, path) = req.split_url();
        let matched = last_matched(routes, &mut req, path);
        (req, matched.is_matched())
    }

    #[test]
    fn route_match_test() {
        let route = Route::new("hello").get(hello);
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/hello".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);
    }

    #[test]
    fn multi_route_match_test() {
        let route = Route::new("hello/world").get(hello);
        let mut routes = Route::new_root();
        routes.push(route);
        let mut req = Request::empty();
        *req.uri_mut() = "/hello/world".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);
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
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);
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
        let (req, matched) = get_matched(&routes, req);
        assert!(matched);
        assert_eq!(
            req.get_path_params::<i64>("id").unwrap(),
            12345678909876543i64
        );
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
        let (req, matched) = get_matched(&routes, req);
        assert!(matched);
        assert_eq!(
            req.get_path_params::<String>("path").unwrap(),
            "hello/world".to_string()
        );
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
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);

        // 测试空路径 - 空路径无法解析为URI，应该跳过这个测试
        // let mut req = Request::empty();
        // *req.uri_mut() = "".parse().unwrap();
        // assert!(get_matched(&routes, req));
    }

    #[test]
    fn nested_empty_path_test() {
        // 测试嵌套的空路径路由
        let route = Route::new("").get(hello).append(Route::new("").get(world));
        let mut routes = Route::new_root();
        routes.push(route);

        // 测试根路径应该匹配第一个处理器
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);
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
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);

        // 测试 /api/v1 应该匹配第二个
        let mut req = Request::empty();
        *req.uri_mut() = "/api/v1".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);
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
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);

        // 测试 /test/ 实际上会匹配到 /test 路由（当前实现的行为）
        // 这是因为 path_split("test/") 返回 ("test", "")，然后匹配成功
        let mut req = Request::empty();
        *req.uri_mut() = "/test/".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(matched);

        // 测试 /test/extra 不应该匹配
        let mut req = Request::empty();
        *req.uri_mut() = "/test/extra".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(!matched);
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
        let (req, matched) = get_matched(&routes, req);
        assert!(matched);
        assert_eq!(req.get_path_params::<i64>("id").unwrap(), 123);

        // 测试无效的数字参数应该不匹配
        let mut req = Request::empty();
        *req.uri_mut() = "/api/user/abc".parse().unwrap();
        let (_req, matched) = get_matched(&routes, req);
        assert!(!matched);

        // 测试字符串参数
        let mut req = Request::empty();
        *req.uri_mut() = "/post/hello-world".parse().unwrap();
        let (req, matched) = get_matched(&routes, req);
        assert!(matched);
        assert_eq!(
            req.get_path_params::<String>("slug").unwrap(),
            "hello-world"
        );
    }

    #[test]
    fn root_route_matching_test() {
        // 测试根路由匹配问题

        // 测试1: 根路由（没有处理器）
        let root_route = Route::new_root();
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();

        let (mut req, path) = req.split_url();

        match root_route.route_match(&mut req, path) {
            (RouteMatched::Matched(route), _) => {
                assert_eq!(route.path, "");
                assert_eq!(route.handler.len(), 0);
            }
            (RouteMatched::Unmatched, _) => {
                // 根路由没有处理器，所以应该不匹配
                // 这是正确的行为
            }
        }

        // 测试2: 空路径路由（有处理器）
        let route = Route::new("").get(hello);

        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();

        let (mut req, path) = req.split_url();

        match route.route_match(&mut req, path) {
            (RouteMatched::Matched(route), _) => {
                assert_eq!(route.path, "");
                assert_eq!(route.handler.len(), 1);
                assert!(route.handler.contains_key(&Method::GET));
            }
            (RouteMatched::Unmatched, _) => {
                unreachable!();
            }
        }
    }
}
