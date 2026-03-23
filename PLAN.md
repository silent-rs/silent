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

### v2.14（已完成 ✅）

- **常用中间件补充** ✅
  - RateLimiter 中间件（路由/API 级别限流）
  - Compression 中间件（动态响应 gzip/brotli 压缩）
  - RequestId 中间件（scru128 生成请求追踪 ID）

- **OpenAPI 宏系统增强** ✅
  - 复杂请求/响应类型文档化
  - 枚举变体文档生成
  - 与提取器（Path、Query、Json 等）自动集成

- **依赖版本更新** ✅
  - scru128 非可选化、tokio 1.50、chrono 0.4.44

- **低覆盖率模块测试补全** ✅
  - +22 个测试，总计 1717

### v2.15（已完成 ✅）

- **TestClient 集成测试工具** ✅ (#183)
  - TestClient / TestRequest 请求构建器（支持全 HTTP 方法）
  - TestResponse 响应包装器（status/headers/bytes/text/json）
  - 链式断言方法（assert_status/assert_header/assert_body_contains）
  - JSON/Form/Text 请求体支持

- **路由性能优化** ✅ (#184)
  - freeze 模式预构建 Arc<RouteTree>，消除请求级深拷贝
  - SpecialSeg 从 String 优化为 Box<str>
  - 大规模路由表 180x 性能提升

- **Cloudflare Worker 生态增强** ✅ (#185)
  - WorkRoute 新增 with_configs() 方法
  - Context 与 Env 统一通过 Configs 注入
  - 完整的 KV/D1/R2 CRUD 示例
  - 错误状态码正确传递

- **Logger 中间件** ✅ (#186)
  - 结构化 tracing 字段替代位置参数字符串
  - Instant 单调时钟计时
  - 安全获取客户端 IP
  - 区分 4xx(WARN)/5xx(ERROR) 日志级别
  - RequestTimeLogger 标记 deprecated，将在 v2.17.0 移除

## 下一阶段规划

### v2.16 — 框架基础设施增强

目标：补齐框架核心基础设施，提升开发体验和生态互通能力。

- **State 提取器（替代 Configs）**
  - 引入语义明确的 `State<T>` 提取器，用于应用级共享状态
  - Configs 标记 deprecated，提供平滑迁移路径
  - 计划在 v2.18.0 移除 Configs

- **Tower 兼容层**
  - 提供 Tower Service trait 适配器
  - 允许复用 Tower 生态中间件（tower-http 等）

- **OpenAPI 完善**
  - 完善 silent-openapi 宏系统
  - Swagger UI 集成
  - 自动文档生成闭环

- **错误处理增强**
  - 支持自定义错误类型到 HTTP 响应的映射
  - anyhow/thiserror 集成支持
