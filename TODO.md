# TODO（Server 硬化第一阶段：配置统一与连接保护）

> 分支: `feature/server-hardening-quic`（自 `main` 切出）
> 目标版本: v2.13
> 优先级: P0
> 状态: 🟢 已完成当前阶段（M1/M2/M3 基础可观测）
> 验证: cargo check --all / cargo clippy --all-targets --all-features --tests --benches -- -D warnings / cargo nextest run --all-features 已通过（当前分支）

## 目标
- 统一 server 配置入口（限流、超时、请求体大小、ALPN/Alt-Svc 等），提供默认值与覆盖策略
- 为 HTTP/1.1、HTTP/2、HTTP/3/QUIC 提供 per-connection 超时与请求体大小限制
- 改进 listener 公平性与错误退避，避免单个监听器阻塞或忙等
- 增加核心 metrics/tracing 钩子，覆盖 accept/限流/HTTP3/WebTransport/关停等关键路径

## 子任务进度
- ✅ 统一配置入口（ServerConfig/ConnectionLimits）并接入 Server/NetServer/RouteConnection/QUIC
- ✅ per-connection 处理超时、HTTP/1.1-3 请求体大小限制（含 WebTransport 下放至 handler）
- ✅ 监听公平性与错误退避策略（多监听器公平 accept、错误退避/限幅）
- ✅ Metrics/Tracing 钩子（accept/限流/超时/HTTP3/WebTransport/关停，含可选 metrics feature 与示例）

## 下一步（依据 PLAN v2.13-M3 收尾项）
- ✅ QUIC 生产化参数：idle_timeout/max_streams/datagram 默认值与文档已落地（docs/quic-transport.md），QuicTransportConfig 接入监听器。
- ✅ Alt-Svc/ALPN/证书热载：Alt-Svc 自动端口与 ALPN 自定义已提供（Route::with_quic_port + QuicTransportConfig.alpn_protocols），TLS 热载通过 ReloadableCertificateStore 支持，QUIC 证书切换流程与验证方案见 docs/quic-ops.md 与 docs/quic-cert-rotation.md。
- ✅ WebTransport/Datagram 体积/速率限制与观测：WebTransport 会话/帧/Datagram 大小与速率由 ConnectionLimits + WebTransportStream 统一限制，底层 quinn datagram send/recv 已接入；超限/限速时丢弃并计数不中断，会通过 metrics 记录 datagram_dropped/rate_limited。

## 当前待办（QUIC 生产级落地）
- ✅ HTTP/3 请求体流式处理：去除一次性聚合，支持体积上限与读超时（已在 service.rs 内单测验证）。
- ✅ 连接/流保护：并发/限速由 QuicTransportConfig（max_streams）与 ConnectionLimits（WebTransport 会话/帧/Datagram）统一配置，底层 quinn datagram 发送/接收已接入并附带 size/rate 校验；超限/限速时丢弃并计数，不中断会话。
- ✅ 可观测性：已埋 accept/handler/HTTP3/body oversize/WebTransport handshake 指标，补充 session_id/span 字段与 Alt-Svc 命中日志，HTTP3 中间件继承单测已添加；新增 HTTP3 读超时计数、响应字节数直方图以及 WebTransport 会话时长/Datagram 丢弃和限速指标，基本覆盖流建立/处理/关闭与错误场景观测。
- ✅ 配置一致性：HybridListener Alt-Svc 自动对齐 QUIC 端口，ALPN 可通过 QuicTransportConfig 配置；TLS 证书热更新通过 ReloadableCertificateStore 支持，QUIC 证书切换流程见 docs/quic-cert-rotation.md；HTTP/3 路径复用 Route 中间件链与 body 限额，并在 quic/service 测试中验证。
- ✅ 性能与内存：HTTP/3 路径对响应体按固定块大小发送，并在累计一定字节后 `yield`，配合底层流式 body，避免单次大块发送长期占用 executor；HTTP/1.1/2 依赖 hyper/h2 的背压机制。
- ✅ 测试与互操作：在 docs/quic-interop-matrix.md 中补充高 RTT/丢包/0-RTT/迁移等端到端测试矩阵，覆盖 HTTP3/WebTransport/Datagram，并结合 quic-ops/quic-webtransport 提供互操作与回归建议。
- 🟢 示例与文档：新增生产化 WebTransport/HTTP3 示例（examples/quic，带中间件与自定义 WebTransport Handler），补充 TLS/QUIC 证书切换说明与运行指南（quic-ops、examples/quic/README.md）。
  - 🔄 新增 `docs/quic-cert-rotation.md` 描述 QUIC 证书切换完整流程。

## 验收标准
- 新配置结构可同时作用于 TCP/TLS/QUIC，默认值落地，可通过测试或示例验证
- 超时与请求体大小限制在 HTTP/1.1、HTTP/2、HTTP/3 路径均生效，并有验证用例或实验性测试
- listener 退避策略对连续 accept 错误不会忙等，多监听器公平竞争有测试或明确说明
- Metrics/Tracing 埋点清单落实到代码，暴露关键指标与 span 字段（含 peer 与 listener 信息）
- 基础回归通过：至少 `cargo check --all`（必要时特性开关）验证；当前分支已通过 cargo check/clippy/nextest

---

# TODO（安全与稳定性修复）

> 分支: `fix/security-stability`（自 `main` 切出）
> 优先级: P0
> 状态: 🟢 已完成

## 目标
- 移除高风险 `unsafe` 并修复潜在安全漏洞（路径穿越）
- 将库内关键路径的 `panic!/unwrap()` 降级为可控错误返回

## 子任务清单
- ✅ WebSocket：移除 `unsafe impl Sync`，确保线程安全边界清晰（`silent/src/ws/websocket.rs`）
- ✅ Static：修复静态文件处理的路径穿越（`silent/src/handler/static/handler.rs`）
- ✅ Session/Template：关键 `unwrap()` 改为返回 `SilentError`（`silent/src/session/*`、`silent/src/templates/middleware.rs`）
- ✅ Listener：`ListenersBuilder` 绑定/转换失败不再 `panic!`（`silent/src/server/listener.rs`）

## 验收标准
- `cargo fmt -- --check` 通过
- `cargo clippy --all-targets --all-features --tests --benches -- -D warnings` 通过
- 关键路径不再出现新增 `unsafe`/非测试 `panic!/unwrap()`

# TODO（关键路径 unwrap/panic 收敛）

> 分支: `fix/no-unwrap-runtime`（基于 `fix/security-stability` 堆叠）
> 优先级: P0
> 状态: 🟢 已完成

## 目标
- 进一步减少运行时关键路径的 `unwrap()/panic!`，避免生产环境因边界条件崩溃

## 子任务清单
- ✅ Session：合并 CookieJar 时避免 `unwrap()`（`silent/src/session/middleware.rs`）
- ✅ Worker：构造错误响应时避免 `unwrap()`（`silent/src/route/worker.rs`）

## 验收标准
- `cargo fmt -- --check` 通过
- `cargo clippy --all-targets --all-features --tests --benches -- -D warnings` 通过

# TODO（SocketAddr 兼容仅 IP 字符串） ✅ 已完成

> 分支: `fix/socketaddr-ip-only`（自 `main` 切出，示意）
> 目标版本: v2.12
> 优先级: P2
> 状态: ✅ 已完成

## 变更摘要
- 调整 `core::socket_addr::SocketAddr` 的 `FromStr` 实现
- 支持仅包含 IP、未携带端口的地址字符串（例如来自 Nginx 的 `X-Real-IP`）
- 当仅提供 IP 时，内部统一转换为端口为 `0` 的 TCP 地址

## 修改的文件
- `silent/src/core/socket_addr.rs`

## 验收标准
- [x] `"127.0.0.1".parse::<silent::SocketAddr>()` 可成功返回 `SocketAddr`
- [x] 仍兼容原有 `ip:port` 与 Unix Socket 路径解析
- [x] `cargo fmt --all` 通过
- [x] `cargo check -p silent --all-features` 通过
