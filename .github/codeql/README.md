# CodeQL 配置说明

## 概述

此配置文件用于配置 GitHub 的 CodeQL 安全扫描，以排除测试代码和示例代码的扫描，减少误报。

## 配置文件

- `codeql-config.yml`: CodeQL 主配置文件

## 排除路径

以下路径被排除在 CodeQL 扫描之外：

- `examples/**`: 示例代码
- `benches/**`: 基准测试代码
- `benchmark/**`: 性能测试代码
- `**/*_test.rs`: 测试文件
- `**/tests/**`: 测试目录
- `**/test*.rs`: 测试相关文件

## 扫描路径

仅扫描以下路径：

- `silent/src/**`: 框架核心源代码

## Cookie 安全警告说明

### 误报说明

CodeQL 可能会在测试代码中报告 "Cookie attribute 'Secure' is not set to true" 的警告。这是一个**误报**，原因如下：

1. **测试代码中的 Cookie**：
   - 测试代码使用 `Cookie::new()` 创建测试用的 cookie
   - 这些 cookie 仅用于单元测试，不会发送到实际的 HTTP 响应中
   - 测试代码需要简洁，不需要设置所有安全属性

2. **生产代码中的 Cookie**：
   - 实际生产代码中，session cookie 已正确设置 `Secure` 属性
   - 参见：[`silent/src/session/middleware.rs:85-89`](../../silent/src/session/middleware.rs#L85-L89)
   ```rust
   cookies.add(
       Cookie::build(("silent-web-session", cookie_value))
           .max_age(cookie::time::Duration::hours(2))
           .secure(true),  // ✅ 已设置 Secure 属性
   );
   ```

### 如何处理误报

如果 CodeQL 仍然报告此警告：

1. **检查警告位置**：确认是在测试代码中（`#[cfg(test)]` 或 `#[test]` 标记）
2. **查看生产代码**：验证 `session/middleware.rs` 中已正确设置 `Secure` 属性
3. **标记为误报**：在 GitHub Security 中将该警告标记为 "False Positive"

### 安全最佳实践

对于使用 Silent 框架的开发者：

```rust
// ✅ 推荐：使用 Cookie::build() 并设置安全属性
Cookie::build(("session", session_id))
    .secure(true)        // HTTPS only
    .http_only(true)     // 防止 XSS
    .same_site(cookie::SameSite::Lax)  // CSRF 防护
    .max_age(Duration::hours(2))
    .finish();

// ❌ 不推荐：仅用于测试，不要在生产环境使用
Cookie::new("session", session_id)
```

## 相关链接

- [CodeQL 文档](https://codeql.github.com/docs/)
- [GitHub Advanced Security 配置](https://docs.github.com/en/code-security/code-scanning/automatically-scanning-your-code-for-vulnerabilities-and-errors/configuring-code-scanning)
