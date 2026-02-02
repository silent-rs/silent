pub(crate) mod cookie_ext;
pub(crate) mod middleware;

// 包含独立测试文件
// 使用 #[cfg(test)] 确保只在测试时编译
// 测试文件位于 tests/ 目录以避免安全扫描工具误判
#[cfg(test)]
mod tests;
