# RFC: 解耦网络监听与协议处理

- 日期：2025-10-01
- 作者：silent-mqtt 团队
- 状态：Draft

## 背景

当前 `silent` 框架的网络监听能力主要内聚在 `service::Server` 之中，面向 HTTP/Hyper 协议的场景。像 `silent-mqtt` 这类非 HTTP 协议项目，如果希望复用 `silent` 的监听与连接抽象，只能直接引入 `core::listener::ListenersBuilder` 等内部实现细节。这会导致如下问题：

1. 业务代码需了解 `silent` 内部模块组织，耦合度过高。
2. 难以通过 `silent` 提供的统一入口实现自定义协议握手，因为监听逻辑与 Hyper 服务绑定在一起。
3. 一旦底层监听实现调整，所有依赖内核模块的下游项目都需要同步修改。

## 目标

- 暴露一个与具体协议无关的网络监听服务，供任意协议栈复用。
- 让 `silent-mqtt` 等项目仅关注协议解析与状态管理，通过组合 `silent` 网络服务器即可提供完整服务。
- 保持现有 HTTP 场景兼容，不破坏现有 `service::Server` 行为。

## 设计概述

在 `service` 模块内整合轻量网络服务器能力，提供 `NetServer` 与连接处理抽象 `ConnectionService`：

- `NetServer` 负责基于 `ListenersBuilder` 管理监听套接字，并在 Tokio 运行时内循环接受连接，同时作为 `service::Server` 的底层监听实现复用；`Server::serve_with_connection_handler` 允许注入自定义协议层。
- `ConnectionService` 是一个泛型服务 trait，定义 `call(stream, peer)` 异步处理逻辑，返回 `Result<(), BoxError>`。
- 通过 blanket impl，普通闭包即可充当连接处理器，业务层不必手写结构体实现。
- 所有接受到的连接都会被 `tokio::spawn` 分发到独立任务，错误通过 `tracing::error!` 记录。
- 构建封装化的连接限流器（令牌桶）与优雅关停钩子，保证在高并发场景下的稳定性。

同时开放原本 `pub(crate)` 的 `ListenersBuilder`、`Listeners` 结构，并调整 `listeners.local_addrs()` 的返回类型为切片，以便对外 API 安全地暴露监听端口；相关实现文件迁移至 `service`，形成独立的 Server 子系统。

## API 变更

### 新增

- `silent::service::{NetServer, ConnectionService, BoxedConnection, BoxError, ConnectionFuture}` 作为通用网络监听与连接处理入口。

### 调整

- `silent::core::listener` 改为薄封装，真实实现位于 `service::listener`，配合 `ListenersBuilder`、`Listeners` 对外公开。
- `Listeners::local_addrs()` 返回类型由 `&Vec<SocketAddr>` 改为 `&[SocketAddr]`，避免暴露可变容器实现。
- `silent::lib` 对外 re-export：`NetServer` 及相关别名可直接通过 `silent::NetServer` 使用。

## 兼容性

- 现有依赖 `ListenersBuilder` 的内部模块可以继续工作；对外开放不会破坏既有 API。
- HTTP `Server::serve` 逻辑保持既有中间件与配置注入，但通过内部通用循环复用 `NetServer`，并对外暴露 `serve_with_connection_handler` 供其他协议构建服务。
- `service` 模块内部整合网络循环逻辑，仅在启用 `server` 特性时编译，保持整体依赖结构稳定。

## 实施计划

1. 暴露监听结构体，将监听与网络循环能力迁移至 `service` 模块，并提供 `NetServer` 与通用接口，`Server::serve` 内部复用同一实现。
2. 在 `NetServer` 层引入令牌桶限流参数（默认可配置），提供 `with_rate_limiter` 接口，限制单位时间内可接受的连接数。
3. 为 `NetServer` 和 `Server` 增加优雅关停路径：
   - 注册 `graceful_shutdown()` API，触发停止接收新连接并等待活动连接完成。
   - 支持设置最大等待时长，超时后强制关闭残留任务。
4. 更新 `silent-mqtt` 等使用方，改用 `NetServer` 并保留原有协议处理逻辑。
5. 在 `silent` 与下游项目中运行 `cargo check` 确认兼容性。
6. 基于使用反馈持续扩展，如更多协议示例或指标监控集成。

## 风险与缓解

- **错误处理不足**：当前仅在日志中输出连接处理失败，后续可根据需要扩展回调或指标上报。
- **任务堆积 / 限流参数设置不当**：若限流阈值过低或处理逻辑阻塞，可能出现大量排队；将通过可观测指标以及动态调整能力缓解。
- **优雅关停超时风险**：当活动连接迟迟未释放时，需在文档中明确强制关闭行为，避免无限等待。
- **API 稳定性**：初版主要满足 MQTT 场景，若其他协议出现新增需求，将通过 RFC 迭代完善。

## 成功判定

- `silent-mqtt` 项目能够完全依赖 `silent::NetServer` 启动服务，无需直接访问 `core::listener`。
- 现有 `silent` HTTP 服务行为保持一致，`cargo check` 与集成测试全部通过。
