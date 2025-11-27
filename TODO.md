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
- 🟡 Alt-Svc/ALPN/证书热载：Alt-Svc 自动端口与 ALPN 自定义已提供，TLS 热载已通过 ReloadableCertificateStore + tls_with_reloadable 支持（docs/quic-ops.md），QUIC 证书仍需重建 listener（已补切换验证流程）。
- 🟡 WebTransport/Datagram 体积/速率限制与观测：size/rate 占位与 metrics 已接入（core.rs/service.rs），需对接底层 datagram send/recv API 并补观测验证。

## 当前待办（QUIC 生产级落地）
- ✅ HTTP/3 请求体流式处理：去除一次性聚合，支持体积上限与读超时（已在 service.rs 内单测验证）。
- ✅ 连接/流保护：并发/限速由 QuicTransportConfig（max_streams）与 ConnectionLimits（WebTransport 会话/帧/Datagram）统一配置，底层 quinn datagram 发送/接收已接入并附带 size/rate 校验；超限/限速时丢弃并计数，不中断会话。
- 🟡 可观测性：已埋 accept/handler/HTTP3/body oversize/WebTransport handshake 指标，已补 session_id/span 字段与 Alt-Svc 命中日志，HTTP3 中间件继承单测已添加；新增 HTTP3 读超时计数与 WebTransport 会话时长直方图，仍需补流关闭/错误/ratelimit 命中等 span 字段与直方图。
- 🟡 配置一致性：HybridListener Alt-Svc 已对齐，ALPN 可配置；TLS 证书热更新已支持，HTTP3 中间件继承验证与 QUIC 热载方案待补。
- 🟡 性能与内存：当前仅在大块响应后 yield，需评估响应分块/写入限速/背压策略。
- 🟡 测试与互操作：补高 RTT/丢包/0-RTT/迁移等端到端矩阵，覆盖 HTTP3/WebTransport/Datagram。
- 🟢 示例与文档：新增生产化 WebTransport/HTTP3 示例（examples/quic，带中间件与自定义 WebTransport Handler），补充 TLS/QUIC 证书切换说明与运行指南（quic-ops、examples/quic/README.md）。
  - 🔄 新增 `docs/quic-cert-rotation.md` 描述 QUIC 证书切换完整流程。

## 验收标准
- 新配置结构可同时作用于 TCP/TLS/QUIC，默认值落地，可通过测试或示例验证
- 超时与请求体大小限制在 HTTP/1.1、HTTP/2、HTTP/3 路径均生效，并有验证用例或实验性测试
- listener 退避策略对连续 accept 错误不会忙等，多监听器公平竞争有测试或明确说明
- Metrics/Tracing 埋点清单落实到代码，暴露关键指标与 span 字段（含 peer 与 listener 信息）
- 基础回归通过：至少 `cargo check --all`（必要时特性开关）验证；当前分支已通过 cargo check/clippy/nextest
