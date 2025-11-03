# 项目规划（PLAN）

## 愿景与总体目标
- 构建可复用、可扩展的 Rust Web/网络服务框架 Silent。
- 分层解耦：协议无关的网络层、路由与中间件层、安全与会话等可选能力。

## 版本里程碑
- v2.11：完成网络层解耦（引入 NetServer、ConnectionService、限流/关停）。
- v2.12：代码结构优化与命名规范统一。

## 当前阶段（v2.12 / 结构优化）
- 任务：将 `service` 模块更名为 `server`，并将所有 Server 相关内容整合至该目录中，命名与职责更清晰。
- 影响范围：
  - 代码：`silent/src/service/*` → `silent/src/server/*`，`crate::service::*` 引用更新为 `crate::server::*`。
  - 对外 API：通过 re-export 保持兼容（`Server`、`NetServer`、`ConnectionService` 等仍从 crate 根导出）。
  - 文档：更新文档与 RFC 中的路径引用。

## 功能优先级
1. 结构更名与编译通过（P0）
2. 文档与示例同步更新（P0）
3. 持续清理历史命名与注释（P1）

## 技术选型与架构
- Rust 2024，Tokio 异步运行时，Hyper/QUIC 可选协议层。
- 特性开关：`server` 控制网络与服务器相关模块的编译。

## 关键时间节点
- 2025-11-01：完成模块更名与引用修正，通过 `cargo check`。
