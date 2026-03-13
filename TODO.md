# TODO（v2.14 开发计划）

> 目标版本: v2.14+
> 优先级: P1
> 状态: 规划中

## 上一阶段成果（v2.13 已完成 ✅）

- 统一配置入口（`ServerConfig` / `ConnectionLimits`）
- 监听器公平调度 + 错误退避
- metrics/tracing 全链路可观测
- QUIC/HTTP3 参数全部可配置化
- 测试覆盖率 89.01%（1587 个测试）
- HTTP/1.1/2 请求路径 tracing span（peer/method/uri）
- H3 响应分块参数可配置（`h3_chunk_size`、`h3_yield_bytes`）

## 待开发任务

### P1：常用中间件补充

- [ ] RateLimiter 中间件 — 路由/API 级别限流（区别于连接级 `RateLimiterConfig`）
- [ ] Compression 中间件 — 动态响应体 gzip/brotli 压缩
- [ ] RequestId 中间件 — 使用 scru128 为每个请求生成追踪 ID

### P1：OpenAPI 宏增强

- [ ] 支持复杂请求/响应类型文档化
- [ ] 支持枚举变体文档生成
- [ ] 与提取器（Path、Query、Json）自动集成

### P2：依赖更新

- [ ] scru128 3.2.3 → 最新版本
- [ ] tokio → 最新 1.x
- [ ] chrono → 最新 0.4.x

### P2：低覆盖率模块测试补全

- [ ] `grpc/service.rs` — 67.44%，目标 75%+
- [ ] `route/handler_append.rs` — 59.16%，分析不可测路径
- [ ] `templates/middleware.rs` — 71.95%，目标 85%+

### P3：架构优化

- [ ] 路由性能优化（参数路由匹配预编译，减少内存分配）
- [ ] TestClient 集成测试工具
- [ ] Cloudflare Worker 生态增强（文档、KV/D1/R2 示例）
