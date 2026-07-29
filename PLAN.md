# 项目规划

## 愿景与边界

Silent 是纯 Web 框架，目标是在保持接口清晰和高性能的前提下，提供可靠的 Web 协议基础。

核心职责：

- HTTP/1.1、HTTP/2、HTTP/3 与服务端传输；
- 路由、中间件、请求、响应和通用提取器；
- WebSocket、SSE 与流式响应的协议基础；
- TestClient、静态资源、Cookie、TLS、QUIC 和稳定扩展入口。

以下能力不进入核心：认证与权限、生产会话存储、监控导出器、可靠任务调度、数据库集成、管理界面、MQTT 实现和项目脚手架。它们由独立仓库自行维护，Silent 只提供必要的公共接入接口。

## 版本与兼容原则

- 2.x 不删除现有公开入口、Cargo feature 或公开重导出类型。
- 已迁出核心的旧能力在 2.x 只做必要修复和迁移提示，不继续扩大功能。
- 独立替代项目和迁移指南完成后，才能安排弃用；破坏性移除集中到 3.0。
- 独立项目只能依赖 Silent 公共接口，不得访问私有模块。
- 新增标识统一使用 scru128；时间字段默认使用本地时间。

## 已完成里程碑

### v2.13

- 统一 `ServerConfig`、连接保护、监听公平性和错误退避。
- 完善 HTTP/3、WebTransport、TLS 重载和服务端观测基础。

### v2.14

- 增加限流、压缩、RequestId 等常用中间件。
- 增强 OpenAPI 宏系统并补齐低覆盖率模块测试。

### v2.15

- 增加 TestClient、路由冻结与性能优化、Cloudflare Worker 接入和 Logger 中间件。
- `RequestTimeLogger` 已弃用，但在整个 2.x 保留，最早于 3.0 移除。

### v2.16 / v2.16.1

- 增加 `State` 提取器、Tower 兼容层、OpenAPI 完善和错误响应扩展。
- 完成 PR #198 的热路径优化：路由树共享、中间件快速路径、未匹配零分配、发布配置和 tracing 编译控制。
- `async_trait`、Hyper Future 装箱和公开参数容器调整因收益或兼容约束暂缓，不再作为当前 TODO。

## 当前与后续里程碑

### v2.16.2 — 路线校准与维护

关联 Issue：#222、#223。

- 统一 README、PLAN、TODO、需求整理和版本说明中的项目边界。
- 修正过期路径、OpenAPI 版本组合和 2.x 移除承诺。
- 说明生态项目由独立仓库维护，核心仓库不再追踪其交付进度。
- 只合入已有维护更新和必要安全修复，不加入新路线功能。

完成门槛：文档描述一致，格式、构建、测试和依赖检查通过。

### v2.17 — 安全与扩展基础

按单一职责拆分为独立 TODO、分支和 PR：

1. `fix/trusted-proxy-source-address`：默认不信任来源地址头，区分底层地址和可信客户端地址；
2. `feature/connection-service-context`：增加服务准备、连接上下文、取消信号和每个 Server 的独立配置；
3. `fix/websocket-session-supervisor`：统一监督收发和关闭，清理不可追踪的后台任务；
4. `fix/websocket-bounded-send-queue`：发送队列默认 64 条、满时等待，并提供拒绝、丢弃和关闭策略；
5. `feature/server-lifecycle-hooks`：提供监听成功、开始服务、开始关闭和关闭完成入口；
6. `feature/route-compile-diagnostics`：启动前检查重复路由、非法参数和路由遮蔽；
7. `feature/route-metadata-view`：提供规范化路由和只读路由目录；
8. `feature/extractor-error-mapping`：统一提取失败分类和自定义响应入口；
9. `feature/test-response-streaming`：增加不预读正文的流式测试能力；
10. `feature/realtime-cancellation`：统一 WebSocket、SSE 和普通流式响应的断开信号。

以上顺序固定，分别归入 #209、#210、#212、#213、#214 的对应核心能力，不建立覆盖整个 Issue 的大分支。

完成门槛：旧公开用法保持可用，外部组件无需访问私有模块，慢连接和伪造来源地址均有边界验证。

### v2.18 — Web 协议完整性

- #211：内容协商、请求体解压、条件请求、单范围请求、预压缩文件、缓存控制和 SPA 回退；
- #212：HEAD/OPTIONS/405、命名路由、URL 生成、SCRU128 路径参数和 Host 路由；
- #214：Bytes、Text、Json、Form 的共享缓存、独占流式请求体和公开提取器描述；
- #209：心跳、超时、SSE 断开、服务关闭和基础观测；
- #213：状态化测试会话、Cookie、重定向、multipart、SSE、WebSocket 和真实网络模式评估。

完成门槛：HTTP/1.1、HTTP/2、HTTP/3 行为一致，路由性能回退不超过正常测试噪声外的 3%。

### 3.0 — 兼容迁移

关联总追踪 Issue：#223。

- 在公开 RFC 中确定 admin、security、template 和 grpc 等现有可选能力的长期归属，不预先承诺迁出或移除。
- session、scheduler 等迁出候选仅在替代能力、迁移指南、兼容验证和最后一个 2.x 弃用通知全部完成后移除旧入口。

## 开发规则

- 开发前核对本文件和 `docs/requirements.md`，并把当前单一任务写入 `TODO.md`。
- 每个 TODO 从最新 `main` 创建独立分支，完成后通过 PR 合并。
- 核心 PR 必须通过格式检查、全工作区检查、静态检查、全功能测试和依赖审计。
- 不向已有公开字段的配置结构直接添加字段，不向可被外部穷举的公开枚举直接增加变体。
- #223 只负责总路线；生态条目保留为长期方向，不作为 Silent 核心版本发布门槛。
