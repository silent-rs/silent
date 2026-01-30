use crate::{Request, Response};
use async_session::Session;
use http_body::Body;
use serde::de::DeserializeOwned;

pub trait SessionExt {
    /// Get `Session` reference.
    fn sessions(&self) -> Session;
    /// Get `Session` mutable reference.
    fn sessions_mut(&mut self) -> &mut Session;
    /// Get `Session` from session.
    fn session<V: DeserializeOwned>(&self, name: &str) -> Option<V>;
}

impl SessionExt for Request {
    fn sessions(&self) -> Session {
        self.extensions().get().cloned().unwrap_or_default()
    }

    fn sessions_mut(&mut self) -> &mut Session {
        if self.extensions_mut().get::<Session>().is_none() {
            self.extensions_mut().insert(Session::default());
        }
        self.extensions_mut().get_mut().unwrap()
    }

    fn session<V: DeserializeOwned>(&self, name: &str) -> Option<V> {
        self.sessions().get(name.as_ref())
    }
}

impl<B: Body> SessionExt for Response<B> {
    fn sessions(&self) -> Session {
        self.extensions().get().cloned().unwrap_or_default()
    }

    fn sessions_mut(&mut self) -> &mut Session {
        if self.extensions_mut().get::<Session>().is_none() {
            self.extensions_mut().insert(Session::default());
        }
        self.extensions_mut().get_mut().unwrap()
    }

    fn session<V: DeserializeOwned>(&self, name: &str) -> Option<V> {
        self.sessions().get(name.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_session::Session;

    // ==================== Request SessionExt 测试 ====================

    #[test]
    fn test_request_sessions_default() {
        // 测试没有 session 时返回默认 Session
        let req = Request::empty();
        let session = req.sessions();
        // 验证可以获取 session
        let _val = session.get::<String>("nonexistent");
    }

    #[test]
    fn test_request_sessions_with_existing_session() {
        // 测试获取已存在的 session
        let mut req = Request::empty();
        let mut session = Session::new();
        session.insert("user_id", "12345").unwrap();
        req.extensions_mut().insert(session);

        let retrieved_session = req.sessions();
        assert_eq!(
            retrieved_session.get::<String>("user_id"),
            Some("12345".to_string())
        );
    }

    #[test]
    fn test_request_sessions_mut_creates_if_not_exists() {
        // 测试 sessions_mut 在 session 不存在时创建新的
        let mut req = Request::empty();
        let session_mut = req.sessions_mut();
        // 验证可以插入数据
        session_mut.insert("test", "value").unwrap();
        assert_eq!(session_mut.get::<String>("test"), Some("value".to_string()));
    }

    #[test]
    fn test_request_sessions_mut_returns_existing() {
        // 测试 sessions_mut 返回已存在的 session
        let mut req = Request::empty();
        let session1 = req.sessions_mut();
        session1.insert("key", "value1").unwrap();

        let session2 = req.sessions_mut();
        assert_eq!(session2.get::<String>("key"), Some("value1".to_string()));
    }

    #[test]
    fn test_request_session_get_value() {
        // 测试从 session 中获取特定类型的值
        let mut req = Request::empty();
        let session = req.sessions_mut();
        session.insert("user_id", 12345i32).unwrap();
        session.insert("username", "test_user").unwrap();

        assert_eq!(req.session::<i32>("user_id"), Some(12345));
        assert_eq!(
            req.session::<String>("username"),
            Some("test_user".to_string())
        );
        assert_eq!(req.session::<String>("nonexistent"), None);
    }

    #[test]
    fn test_request_session_with_no_session_data() {
        // 测试没有 session 数据时返回 None
        let req = Request::empty();
        assert_eq!(req.session::<String>("any_key"), None);
    }

    // ==================== Response SessionExt 测试 ====================

    #[test]
    fn test_response_sessions_default() {
        // 测试没有 session 时返回默认 Session
        let res = Response::empty();
        let session = res.sessions();
        // 验证可以获取 session
        let _val = session.get::<String>("nonexistent");
    }

    #[test]
    fn test_response_sessions_with_existing_session() {
        // 测试获取已存在的 session
        let mut res = Response::empty();
        let mut session = Session::new();
        session.insert("data", "test_data").unwrap();
        res.extensions_mut().insert(session);

        let retrieved_session = res.sessions();
        assert_eq!(
            retrieved_session.get::<String>("data"),
            Some("test_data".to_string())
        );
    }

    #[test]
    fn test_response_sessions_mut_creates_if_not_exists() {
        // 测试 sessions_mut 在 session 不存在时创建新的
        let mut res = Response::empty();
        let session_mut = res.sessions_mut();
        // 验证可以插入数据
        session_mut.insert("test", "value").unwrap();
        assert_eq!(session_mut.get::<String>("test"), Some("value".to_string()));
    }

    #[test]
    fn test_response_sessions_mut_returns_existing() {
        // 测试 sessions_mut 返回已存在的 session
        let mut res = Response::empty();
        let session1 = res.sessions_mut();
        session1.insert("response_key", "response_value").unwrap();

        let session2 = res.sessions_mut();
        assert_eq!(
            session2.get::<String>("response_key"),
            Some("response_value".to_string())
        );
    }

    #[test]
    fn test_response_session_get_value() {
        // 测试从 response session 中获取特定类型的值
        let mut res = Response::empty();
        let session = res.sessions_mut();
        session.insert("status", "success").unwrap();
        session.insert("count", 42i32).unwrap();

        assert_eq!(res.session::<String>("status"), Some("success".to_string()));
        assert_eq!(res.session::<i32>("count"), Some(42));
        assert_eq!(res.session::<String>("missing"), None);
    }

    #[test]
    fn test_response_session_with_no_session_data() {
        // 测试没有 session 数据时返回 None
        let res = Response::empty();
        assert_eq!(res.session::<String>("any_key"), None);
    }

    // ==================== Session 数据类型测试 ====================

    #[test]
    fn test_session_with_complex_types() {
        // 测试存储复杂类型
        let mut req = Request::empty();
        let session = req.sessions_mut();

        // 存储字符串
        session.insert("str_val", "hello").unwrap();

        // 存储整数
        session.insert("int_val", 42i64).unwrap();

        // 存储布尔值
        session.insert("bool_val", true).unwrap();

        assert_eq!(req.session::<String>("str_val"), Some("hello".to_string()));
        assert_eq!(req.session::<i64>("int_val"), Some(42));
        assert_eq!(req.session::<bool>("bool_val"), Some(true));
    }

    #[test]
    fn test_request_and_response_session_independence() {
        // 测试 Request 和 Response 的 session 是独立的
        let mut req = Request::empty();
        let mut res = Response::empty();

        req.sessions_mut().insert("req_key", "req_value").unwrap();
        res.sessions_mut().insert("res_key", "res_value").unwrap();

        assert_eq!(
            req.session::<String>("req_key"),
            Some("req_value".to_string())
        );
        assert_eq!(req.session::<String>("res_key"), None);

        assert_eq!(
            res.session::<String>("res_key"),
            Some("res_value".to_string())
        );
        assert_eq!(res.session::<String>("req_key"), None);
    }

    #[test]
    fn test_session_overwrite() {
        // 测试覆盖 session 中的值
        let mut req = Request::empty();
        let session = req.sessions_mut();

        session.insert("key", "value1").unwrap();
        assert_eq!(session.get::<String>("key"), Some("value1".to_string()));

        session.insert("key", "value2").unwrap();
        assert_eq!(session.get::<String>("key"), Some("value2".to_string()));
    }
}
