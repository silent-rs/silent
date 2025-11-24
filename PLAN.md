# 项目规划

## 愿景与目标
- 提升 server 模块生产级能力：统一配置入口、连接保护、公平监听、可观测性、QUIC 落地。
- 提供可运维化的默认配置与可观察行为，降低生产部署风险。

## 版本里程碑
- v2.13-M1：统一配置入口 + 连接保护（per-connection 超时、请求体大小、限流默认值）。
- v2.13-M2：监听公平性与错误退避（多监听器公平 accept、错误退避、健康检测）。
- v2.13-M3：可观测性与 QUIC 生产化（指标/tracing 埋点、Alt-Svc/ALPN 对齐、QUIC 参数与 WebTransport 限制）。

## 优先级（当前阶段）
1. 统一配置入口与连接保护
2. 监听公平性与错误退避
3. metrics/tracing 钩子与 QUIC 生产落地

## 范围与验收要点
- 统一配置入口：集中 server 配置（TCP/TLS/QUIC）默认值与覆盖策略，保持向后兼容。
- 连接保护：per-connection 读/写/总超时、请求体大小限制，HTTP/1.1、HTTP/2、HTTP/3 一致；限流默认值可配置。
- 监听公平/退避：监听器独立 accept 任务或公平调度，连续错误退避（指数/限幅），避免单监听器阻塞全局；可观测 accept 错误。
- 可观测性：tracing span 携带 peer/listener 信息；指标覆盖 accept 成功/失败/退避、限流命中、请求耗时、QUIC/WebTransport 会话与帧计数、关停耗时。
- QUIC 生产化：quinn transport 参数（idle_timeout、max_streams、窗口、max_datagram_size）、WebTransport/HTTP3 帧/体积/速率限制、Alt-Svc 自动端口匹配、ALPN 配置、证书配置/热载说明。

## 技术选型与实现方向
- 配置：集中配置结构，支持默认值与 builder/serde 覆盖；面向 TCP/TLS/QUIC 统一入口。
- 超时/大小限制：Tokio 超时包装 + body 限流（HTTP1/2）、HTTP/3 流式聚合限制；请求体大小限制前置。
- 监听公平：为每个监听器独立 accept 任务 + select/JoinSet；错误退避使用 tokio::time::sleep + 指数退避限幅。
- 可观测性：`tracing` span/field，指标通过 metrics/otel 兼容接口暴露（不依赖具体后端）。
- QUIC：基于 quinn + rustls，ALPN 支持 h3/h3-29；Alt-Svc 端口自动对齐；WebTransport/Datagram 限制与 backpressure 处理。

## 关键时间节点
- 2025-12-01：完成 M1（配置统一 + 连接保护）并验证
- 2025-12-15：完成 M2（监听公平/退避）并验证
- 2026-01-05：完成 M3（可观测性 + QUIC 生产化）并验证
