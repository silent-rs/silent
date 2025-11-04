# 项目规划（PLAN）

## 愿景与总体目标
- 构建可复用、可扩展的 Rust Web/网络服务框架 Silent。
- 分层解耦：协议无关的网络层、路由与中间件层、安全与会话等可选能力。

## 版本里程碑
- v2.11：完成网络层解耦（引入 NetServer、ConnectionService、限流/关停）。
- v2.12：代码结构优化与命名规范统一。

## 当前阶段（v2.12 / 结构优化 + 覆盖率）
- 任务：将 `service` 模块更名为 `server`，并将所有 Server 相关内容整合至该目录中，命名与职责更清晰。
- 任务（测试）：提升 `server/quic` 覆盖率，优先覆盖 HTTP/3 请求-响应通路与错误传播（通过可注入的最小 H3 流接口）。
- 影响范围：
  - 代码：`silent/src/service/*` → `silent/src/server/*`，`crate::service::*` 引用更新为 `crate::server::*`。
  - 对外 API：通过 re-export 保持兼容（`Server`、`NetServer`、`ConnectionService` 等仍从 crate 根导出）。
  - 文档：更新文档与 RFC 中的路径引用。

## 专题：QUIC/HTTP/3 稳定化（quichttp3）

- 分支：`feature/quichttp3-stabilization`（自 `main` 切出）
- 目标：
  - 稳定 HTTP/3 请求-响应链路（读取/聚合/发送、错误传播、资源释放）。
  - WebTransport 握手与响应头回传的语义稳定，避免依赖真实网络环境的测试不确定性。
  - 内部抽象收敛：保持 `H3RequestIo` 为文件内私有、最小方法集，避免泄露协议细节到通用网络层。
  - 与路由/中间件的交互行为一致（状态码、头、响应体）。
- 范围：
  - 单测完善：HTTP/3 多帧/空体/错误分支；WebTransport 握手头传递。
  - 代码健壮性：边界日志、错误信息上下文、避免 panic。
  - 覆盖率：server/quic 模块行/函数覆盖率上升（目标：较当前基线提升）。
- 验收标准：
  - `cargo fmt`、`cargo clippy -D warnings`、`cargo check` 通过。
  - `cargo test -p silent --all-features` 全部通过；`cargo llvm-cov nextest` 可产出报告。
  - 不引入对外 API 破坏性变更。
- 风险与对策：
  - H3 外部依赖接口变更 → 通过最小适配层隔离；测试优先使用伪造流。
  - 平台差异导致不稳定用例 → 统一用“占用端口再绑定”式模式替代特权端口假设。
- 时间节点：
  - 分支创建：当前（本次提交）。
  - 用例与收敛迭代：1-2 个工作日内完成并提 PR。

## 功能优先级
1. 结构更名与编译通过（P0）
2. 文档与示例同步更新（P0）
3. 持续清理历史命名与注释（P1）

## 技术选型与架构
- Rust 2024，Tokio 异步运行时，Hyper/QUIC 可选协议层。
- 特性开关：`server` 控制网络与服务器相关模块的编译。

## 关键时间节点
- 2025-11-01：完成模块更名与引用修正，通过 `cargo check`。
