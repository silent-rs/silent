// CookieExt trait 的独立测试文件
//
// 此文件包含所有 cookie_ext.rs 中 CookieExt trait 的单元测试

use cookie::{Cookie, CookieJar};
// 导入 CookieExt trait 和必要类型
use crate::{CookieExt, Request, Response};

// 辅助函数：创建用于测试的安全 Cookie（带 Secure 属性）
fn test_cookie(name: &str, value: impl AsRef<str>) -> Cookie<'static> {
    Cookie::build((name.to_owned(), value.as_ref().to_owned()))
        .secure(true)
        .build()
}

// ==================== Request CookieExt 测试 ====================

#[test]
fn test_request_cookies_default() {
    let request = Request::empty();
    // 没有 CookieJar 时应该返回默认的空 CookieJar
    let jar = request.cookies();
    // 通过检查不存在的 cookie 来验证 jar 为空
    assert!(jar.get("nonexistent").is_none());
}

#[test]
fn test_request_cookies_with_jar() {
    let mut request = Request::empty();
    // 手动插入一个 CookieJar
    request.extensions_mut().insert(CookieJar::new());

    let jar = request.cookies();
    // 应该能获取到插入的 CookieJar
    assert!(jar.get("nonexistent").is_none());
}

#[test]
fn test_request_cookies_mut_initializes() {
    let mut request = Request::empty();
    // 第一次调用应该初始化 CookieJar
    let jar = request.cookies_mut();
    assert!(jar.get("nonexistent").is_none());

    // 第二次调用应该返回同一个 CookieJar
    let _ = jar; // 显式结束第一个可变引用的生命周期
    let jar2 = request.cookies_mut();
    assert!(jar2.get("nonexistent").is_none());
}

#[test]
fn test_request_cookies_mut_persists() {
    let mut request = Request::empty();

    // 在 cookies_mut 中添加一个 cookie
    // 测试代码中使用简化版本，实际生产环境应设置安全属性
    let jar = request.cookies_mut();
    jar.add(test_cookie("test", "value"));

    // 通过 cookies() 应该能获取到
    let jar2 = request.cookies();
    assert_eq!(jar2.get("test").map(|c| c.value()), Some("value"));
}

#[test]
fn test_request_cookie_none() {
    let request = Request::empty();
    // 没有 CookieJar 时应该返回 None
    assert!(request.cookie("test").is_none());
}

#[test]
fn test_request_cookie_some() {
    let mut request = Request::empty();

    // 添加一个 CookieJar 并设置 cookie
    let jar = request.cookies_mut();
    jar.add(test_cookie("session", "abc123"));

    // 应该能获取到这个 cookie
    let cookie = request.cookie("session");
    assert!(cookie.is_some());
    assert_eq!(cookie.unwrap().value(), "abc123");
}

#[test]
fn test_request_cookie_not_found() {
    let mut request = Request::empty();

    // 添加一个 CookieJar
    let jar = request.cookies_mut();
    jar.add(test_cookie("other", "value"));

    // 查找不存在的 cookie 应该返回 None
    assert!(request.cookie("session").is_none());
}

#[test]
fn test_request_cookie_with_string() {
    let mut request = Request::empty();
    let jar = request.cookies_mut();
    jar.add(test_cookie("test", "value"));

    // 使用 &str 查找
    assert!(request.cookie("test").is_some());

    // 使用 String 查找
    let name = String::from("test");
    assert!(request.cookie(name).is_some());
}

// ==================== Response CookieExt 测试 ====================

#[test]
fn test_response_cookies_default() {
    let response = Response::empty();
    // 没有 CookieJar 时应该返回默认的空 CookieJar
    let jar = response.cookies();
    // 通过检查不存在的 cookie 来验证 jar 为空
    assert!(jar.get("nonexistent").is_none());
}

#[test]
fn test_response_cookies_with_jar() {
    let mut response = Response::empty();
    // 手动插入一个 CookieJar
    response.extensions_mut().insert(CookieJar::new());

    let jar = response.cookies();
    // 应该能获取到插入的 CookieJar
    assert!(jar.get("nonexistent").is_none());
}

#[test]
fn test_response_cookies_mut_initializes() {
    let mut response = Response::empty();
    // 第一次调用应该初始化 CookieJar
    let jar = response.cookies_mut();
    assert!(jar.get("nonexistent").is_none());

    // 第二次调用应该返回同一个 CookieJar
    let _ = jar; // 显式结束第一个可变引用的生命周期
    let jar2 = response.cookies_mut();
    assert!(jar2.get("nonexistent").is_none());
}

#[test]
fn test_response_cookies_mut_persists() {
    let mut response = Response::empty();

    // 在 cookies_mut 中添加一个 cookie
    let jar = response.cookies_mut();
    jar.add(test_cookie("test", "value"));

    // 通过 cookies() 应该能获取到
    let jar2 = response.cookies();
    assert_eq!(jar2.get("test").map(|c| c.value()), Some("value"));
}

#[test]
fn test_response_cookie_none() {
    let response = Response::empty();
    // 没有 CookieJar 时应该返回 None
    assert!(response.cookie("test").is_none());
}

#[test]
fn test_response_cookie_some() {
    let mut response = Response::empty();

    // 添加一个 CookieJar 并设置 cookie
    let jar = response.cookies_mut();
    jar.add(test_cookie("session", "xyz789"));

    // 应该能获取到这个 cookie
    let cookie = response.cookie("session");
    assert!(cookie.is_some());
    assert_eq!(cookie.unwrap().value(), "xyz789");
}

#[test]
fn test_response_cookie_not_found() {
    let mut response = Response::empty();

    // 添加一个 CookieJar
    let jar = response.cookies_mut();
    jar.add(test_cookie("other", "value"));

    // 查找不存在的 cookie 应该返回 None
    assert!(response.cookie("session").is_none());
}

#[test]
fn test_response_cookie_with_string() {
    let mut response = Response::empty();
    let jar = response.cookies_mut();
    jar.add(test_cookie("test", "value"));

    // 使用 &str 查找
    assert!(response.cookie("test").is_some());

    // 使用 String 查找
    let name = String::from("test");
    assert!(response.cookie(name).is_some());
}

// ==================== 多 Cookie 操作测试 ====================

#[test]
fn test_request_multiple_cookies() {
    let mut request = Request::empty();
    let jar = request.cookies_mut();

    jar.add(test_cookie("cookie1", "value1"));
    jar.add(test_cookie("cookie2", "value2"));
    jar.add(test_cookie("cookie3", "value3"));

    assert_eq!(request.cookie("cookie1").unwrap().value(), "value1");
    assert_eq!(request.cookie("cookie2").unwrap().value(), "value2");
    assert_eq!(request.cookie("cookie3").unwrap().value(), "value3");
}

#[test]
fn test_response_multiple_cookies() {
    let mut response = Response::empty();
    let jar = response.cookies_mut();

    jar.add(test_cookie("cookie1", "value1"));
    jar.add(test_cookie("cookie2", "value2"));
    jar.add(test_cookie("cookie3", "value3"));

    assert_eq!(response.cookie("cookie1").unwrap().value(), "value1");
    assert_eq!(response.cookie("cookie2").unwrap().value(), "value2");
    assert_eq!(response.cookie("cookie3").unwrap().value(), "value3");
}

// ==================== 边界条件测试 ====================

#[test]
fn test_request_empty_cookie_name() {
    let mut request = Request::empty();
    let jar = request.cookies_mut();
    jar.add(test_cookie("", "value"));

    // 空名称的 cookie
    let cookie = request.cookie("");
    assert!(cookie.is_some());
    assert_eq!(cookie.unwrap().value(), "value");
}

#[test]
fn test_response_empty_cookie_name() {
    let mut response = Response::empty();
    let jar = response.cookies_mut();
    jar.add(test_cookie("", "value"));

    // 空名称的 cookie
    let cookie = response.cookie("");
    assert!(cookie.is_some());
    assert_eq!(cookie.unwrap().value(), "value");
}

#[test]
fn test_request_special_cookie_value() {
    let mut request = Request::empty();
    let jar = request.cookies_mut();
    jar.add(test_cookie("test", "value with spaces"));

    let cookie = request.cookie("test");
    assert_eq!(cookie.unwrap().value(), "value with spaces");
}

#[test]
fn test_response_special_cookie_value() {
    let mut response = Response::empty();
    let jar = response.cookies_mut();
    jar.add(test_cookie("test", "value with spaces"));

    let cookie = response.cookie("test");
    assert_eq!(cookie.unwrap().value(), "value with spaces");
}

#[test]
fn test_cookies_isolation() {
    // Request 和 Response 的 CookieJar 应该独立
    let mut request = Request::empty();
    let mut response = Response::empty();

    request
        .cookies_mut()
        .add(test_cookie("req_cookie", "req_value"));
    response
        .cookies_mut()
        .add(test_cookie("resp_cookie", "resp_value"));

    // Request 不应该有 Response 的 cookie
    assert!(request.cookie("resp_cookie").is_none());
    // Response 不应该有 Request 的 cookie
    assert!(response.cookie("req_cookie").is_none());
}

#[test]
fn test_request_cookies_cloned() {
    let mut request = Request::empty();
    request.cookies_mut().add(test_cookie("test", "value"));

    // cookies() 应该返回克隆的 CookieJar
    let jar1 = request.cookies();
    let jar2 = request.cookies();

    // 两者都应该包含相同的 cookie
    assert_eq!(jar1.get("test").map(|c| c.value()), Some("value"));
    assert_eq!(jar2.get("test").map(|c| c.value()), Some("value"));
}

#[test]
fn test_response_cookies_cloned() {
    let mut response = Response::empty();
    response.cookies_mut().add(test_cookie("test", "value"));

    // cookies() 应该返回克隆的 CookieJar
    let jar1 = response.cookies();
    let jar2 = response.cookies();

    // 两者都应该包含相同的 cookie
    assert_eq!(jar1.get("test").map(|c| c.value()), Some("value"));
    assert_eq!(jar2.get("test").map(|c| c.value()), Some("value"));
}

#[test]
fn test_request_cookies_mut_same_instance() {
    let mut request = Request::empty();

    // cookies_mut() 应该总是返回同一个可变引用
    {
        let jar1 = request.cookies_mut();
        jar1.add(test_cookie("test", "value1"));
    } // 释放第一个可变引用

    {
        let jar2 = request.cookies_mut();
        jar2.add(test_cookie("test2", "value2"));
    } // 释放第二个可变引用

    // 应该都能在同一个 CookieJar 中找到
    assert!(request.cookie("test").is_some());
    assert!(request.cookie("test2").is_some());
}

#[test]
fn test_response_cookies_mut_same_instance() {
    let mut response = Response::empty();

    // cookies_mut() 应该总是返回同一个可变引用
    {
        let jar1 = response.cookies_mut();
        jar1.add(test_cookie("test", "value1"));
    } // 释放第一个可变引用

    {
        let jar2 = response.cookies_mut();
        jar2.add(test_cookie("test2", "value2"));
    } // 释放第二个可变引用

    // 应该都能在同一个 CookieJar 中找到
    assert!(response.cookie("test").is_some());
    assert!(response.cookie("test2").is_some());
}
