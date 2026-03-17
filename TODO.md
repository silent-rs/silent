# TODO（v2.15 开发计划）

> 目标版本: v2.15
> 状态: 开发中

## 上一阶段成果（v2.14 已完成 ✅）

- RateLimiter / Compression / RequestId 三个常用中间件
- OpenAPI 宏增强：提取器自动集成、枚举变体文档、复杂类型文档化
- 依赖更新：scru128 非可选化、tokio 1.50、chrono 0.4.44
- 低覆盖率模块测试补全（+22 个测试，总计 1717）

## 待开发任务

### P1：TestClient 集成测试工具

- [ ] TestClient / TestRequest 请求构建器（支持 GET/POST/PUT/DELETE/PATCH）
- [ ] TestResponse 响应包装器（status/headers/body_bytes/body_string/body_json）
- [ ] 链式断言方法（assert_status/assert_header/assert_body_contains）
- [ ] JSON/Form 请求体支持
- [ ] 中间件和提取器完整测试路径验证

### P2：架构优化

- [ ] 路由性能优化（参数路由匹配预编译，减少内存分配）
- [ ] Cloudflare Worker 生态增强（文档、KV/D1/R2 示例）
