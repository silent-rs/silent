# TODO（Server 硬化第一阶段：配置统一与连接保护）

> 分支: `feature/server-hardening-quic`（自 `main` 切出）
> 目标版本: v2.13
> 优先级: P0
> 状态: 🟡 进行中（已完成：配置统一入口 + 连接超时/请求体大小限制 + 监听公平/退避 + 可选 metrics 埋点与示例）

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

## 验收标准
- 新配置结构可同时作用于 TCP/TLS/QUIC，默认值落地，可通过测试或示例验证
- 超时与请求体大小限制在 HTTP/1.1、HTTP/2、HTTP/3 路径均生效，并有验证用例或实验性测试
- listener 退避策略对连续 accept 错误不会忙等，多监听器公平竞争有测试或明确说明
- Metrics/Tracing 埋点清单落实到代码，暴露关键指标与 span 字段（含 peer 与 listener 信息）
- 基础回归通过：至少 `cargo check --all`（必要时特性开关）验证
