# TODO（Server 硬化第一阶段：配置统一与连接保护）

> 分支: `feature/server-hardening-quic`（自 `main` 切出）
> 目标版本: v2.13
> 优先级: P0
> 状态: 🟢 已完成当前阶段（M1/M2/M3 基础可观测）

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
- 🔄 QUIC 生产化参数：暴露 idle_timeout/max_streams/datagram 上限，提供默认值与文档。
- 🔄 Alt-Svc/ALPN 对齐与证书热载说明。
- 🔄 WebTransport/Datagram 体积/速率限制与观测（计数、直方图）。

## 新增待办（QUIC 生产级落地）
- ✅ HTTP/3 请求体流式处理：去除一次性聚合，支持体积上限与读超时。
- 🟡 连接/流保护：每连接/每流并发、datagram 大小与速率限制；quinn transport 参数默认值与文档化。
- 🟡 可观测性：accept/握手/请求/流关闭/错误的 tracing span 与 metrics（含 rate-limit 命中、流重置、处理耗时、Alt-Svc 命中）。
- 🟡 配置一致性：HybridListener 自动 Alt-Svc 端口；自定义 ALPN；证书热更新；HTTP3 路径继承 HTTP1/2 中间件验证。
- 🟡 性能与内存：响应侧分块/backpressure，避免大响应占用内存。
- 🟡 测试与互操作：端到端回归、丢包/高 RTT、0-RTT/重传/迁移策略说明与验证。
- 🟡 示例与文档：生产化 WebTransport/HTTP3 示例（替代 Echo），列出防护、证书、Alt-Svc、监控的必需配置。

## 验收标准
- 新配置结构可同时作用于 TCP/TLS/QUIC，默认值落地，可通过测试或示例验证
- 超时与请求体大小限制在 HTTP/1.1、HTTP/2、HTTP/3 路径均生效，并有验证用例或实验性测试
- listener 退避策略对连续 accept 错误不会忙等，多监听器公平竞争有测试或明确说明
- Metrics/Tracing 埋点清单落实到代码，暴露关键指标与 span 字段（含 peer 与 listener 信息）
- 基础回归通过：至少 `cargo check --all`（必要时特性开关）验证
