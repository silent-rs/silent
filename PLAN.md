# 项目规划

## 愿景与目标
- 提升 server 模块生产级能力：统一配置入口、连接保护、公平监听、可观测性、QUIC 落地。
- 提供可运维化的默认配置与可观察行为，降低生产部署风险。

## 版本里程碑

### v2.13（已完成 ✅）

- **M1：统一配置入口 + 连接保护** ✅
  - `ServerConfig` / `ConnectionLimits` 统一配置结构（`server/config.rs`）
  - per-connection 超时、请求体大小限制（HTTP/1.1、HTTP/2、HTTP/3 统一）
  - 令牌桶限流器（`RateLimiterConfig`）
  - HTTP/3 分块参数可配置（`h3_chunk_size`、`h3_yield_bytes`）

- **M2：监听公平性与错误退避** ✅
  - 多监听器 Round-robin 公平调度（`listener.rs` Listeners）
  - 每个监听器独立 `BackoffState`，指数退避 + 限幅（50ms → 2s）
  - `JoinSet` + `select!` 并发 accept，单监听器不阻塞全局
  - Accept 错误可观测（`record_accept_err`）

- **M3：可观测性与 QUIC 生产化** ✅
  - metrics 埋点覆盖 accept/限流/handler/关停/QUIC 全链路（`metrics.rs`）
  - tracing span 覆盖 HTTP/1.1/2/3 请求路径，携带 peer/method/uri
  - Alt-Svc 中间件自动端口匹配（`quic/middleware.rs`）
  - ALPN 配置支持 h3/h3-29（`QuicTransportConfig`）
  - quinn transport 参数全部可配置（idle_timeout、max_streams、窗口等）
  - WebTransport 帧/体积/速率限制（`ConnectionLimits`）
  - 证书可重载（`ReloadableTlsListener`）

- **测试覆盖率** ✅
  - 行覆盖率 89.01%，1587 个测试全部通过
  - 三阶段优化全部完成

## 下一阶段规划（v2.14+）

### 优先级排序

1. **P1：常用中间件补充**
   - RateLimiter 中间件（路由/API 级别限流）
   - Compression 中间件（动态响应 gzip/brotli 压缩）
   - RequestId 中间件（scru128 生成请求追踪 ID）

2. **P1：OpenAPI 宏系统增强**
   - 复杂请求/响应类型文档化
   - 枚举变体文档生成
   - 与提取器（Path、Query、Json 等）自动集成

3. **P2：依赖版本更新**
   - scru128、tokio、chrono 等依赖更新

4. **P2：低覆盖率模块测试补全**
   - `grpc/service.rs` (67.44%)
   - `route/handler_append.rs` (59.16%)
   - `templates/middleware.rs` (71.95%)

5. **P3：路由性能优化**
   - 参数路由匹配预编译
   - 减少路径参数提取的内存分配

6. **P3：TestClient 测试工具**
   - 提供内建的集成测试辅助工具

7. **P3：Cloudflare Worker 生态增强**
   - 完善 Worker 集成文档
   - 添加 KV、D1、R2 场景示例
