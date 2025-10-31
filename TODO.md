# TODO - Issue #3: 完成网络层解耦（提取 NetServer 结构）

> 分支: `feature/net-server-decoupling`
> 依据: `PLAN.md` → Phase 1 / P0、`rfcs/2025-10-01-net-server-decoupling.md`
> 目标版本: v2.11
> 最近更新: 2025-10-31

---

## 🎯 目标与范围

- 在 `service` 模块内提供与协议无关的网络监听能力：`NetServer`。
- 定义通用连接处理抽象 `ConnectionService`，支持闭包/结构体实现。
- 现有 `Server` 的 HTTP 能力保持兼容；内部改为复用 `NetServer` 通用循环。
- 将监听相关能力收敛到 `service::listener`，必要类型对外公开与 re-export。
- 提供限流（令牌桶）与优雅关停能力。

不做：
- 不变更现有公开 API 的语义（除新增 re-export/方法外）。
- 不立即实现新协议适配（仅提供自定义协议示例）。

---

## 🔐 小约定（Contract）
- ConnectionService
  - 输入：`(stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static, peer: SocketAddr)`
  - 输出：`Future<Output = Result<(), BoxError>>`
  - 错误：记录日志，不影响 NetServer 主循环；严重错误可触发指标上报（预留）。
- 优雅关停：
  - 停止接受新连接；等待活动任务在超时内完成；超时后强制取消。
- 限流：
  - 令牌桶（QPS 或连接接受速率）；不足时暂缓/丢弃（行为可配置，先实现“暂缓+最大等待”）。

---

## ✅ 验收标准
- 核心类型：`service::NetServer`、`service::ConnectionService` 可用。
- `Server::serve` 保持行为一致；新增 `serve_with_connection_handler`。
- 提供 `examples/net_server_basic` 与 `examples/net_server_custom_protocol`（最小可运行）。
- 单元/集成测试覆盖：限流、关停、基本连接分发。
- 文档：模块级与公共 API rustdoc 完整；RFC 状态更新。
- 质量门禁：`fmt`/`clippy`/`check`/`nextest`/`deny` 通过。

---

## 🧩 任务分解

### 1) 设计对齐（文档）
- [x] 细化 `ConnectionService` trait 签名与别名（`BoxError`、`ConnectionFuture`）。
- [x] `NetServer` 构造参数与运行接口（监听源、限流器、关停句柄）。
- [x] 迁移/公开 `ListenersBuilder`、`Listeners` 的最小集合与 `local_addrs()` 返回 `&[SocketAddr]`。
- [x] 最小 PoC 时序图（接入 → 分发 → 关停）。

### 2) 实现（service 模块）
- [x] `service/net_server.rs`：
  - [x] `NetServer` 结构体、`run()`/`serve()` 主循环（tokio::spawn 分发）。
  - [ ] `with_rate_limiter()` / `with_shutdown()` / 构造器。
  - [ ] 错误处理与 `tracing` 记录（细化错误语义与文档）。
- [ ] `service/connection_service.rs`：
  - [ ] `ConnectionService` trait + blanket impl（闭包 → 服务）。
  - [ ] 别名类型：`BoxError`、`ConnectionFuture`。
- [ ] `service/listener.rs`：
  - [ ] 收敛监听能力，公开必要类型，`local_addrs() -> &[SocketAddr]`。
  - [ ] 更新内部依赖处的调用点。
- [x] `service/server.rs`：
  - [x] `Server::serve` 内部改为复用 `NetServer`。
  - [ ] 新增/整理 `serve_with_connection_handler()`（与可见性一致）。
- [ ] `lib.rs` re-export：`NetServer` / `ConnectionService` / 相关别名。

### 3) 限流与关停
- [ ] 令牌桶实现（简单版）：容量/速率/补充间隔参数。
- [ ] 限流策略：等待队列上限与超时策略。
- [ ] 优雅关停：停止 accept + 等待活动任务 + 超时强制取消。

### 4) 示例
- [ ] `examples/net_server_basic/`：
  - [ ] 监听 TCP，回显字节数或简单问候。
  - [ ] 展示 `with_rate_limiter()` 与关停示例。
- [ ] `examples/net_server_custom_protocol/`：
  - [ ] 假协议（如行分隔命令），展示自定义 handler。

### 5) 测试
- 单元测试：
  - [ ] 限流器：补充与消耗、等待/超时路径。
  - [ ] 关停：超时前完成/超时强制取消。
  - [ ] listener 的 `local_addrs()` 只读视图。
- 集成测试：
  - [ ] 启动 `NetServer`，发起连接，验证处理函数被调用。
  - [ ] `Server::serve_with_connection_handler` 的兼容路径。

### 6) 文档与示例文档
- [ ] 为新增公共 API 添加 rustdoc（含 `# Examples`、`# Errors`、`# Panics`）。
- [ ] 更新 `rfcs/2025-10-01-net-server-decoupling.md` 状态为 Implementing。
- [ ] 在 README 的特性列表中加入 “通用网络层（NetServer）”。

### 7) 质量门禁
- [ ] `cargo fmt -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo nextest run --all-features`
- [ ] `cargo deny check`（注意第三方依赖告警，必要时在 `deny.toml` 标注例外并附注释）
- [ ] 运行 `scripts/coverage.sh` 获取新增代码的基本覆盖情况

### 8) 兼容性与迁移
- [ ] 全量示例编译通过（`examples/*`）。
- [ ] 现有 HTTP `Server` 行为不变（路由/中间件/配置）。
- [ ] 若 `local_addrs()` 签名变更影响示例或下游，逐一修正并记录迁移说明（如必要）。

---

## 🧪 边界与风控
- 连接洪峰：限流策略不当导致排队堆积 → 提供上限与丢弃策略开关（暂留 TODO）。
- 关停卡死：活动任务不退出 → 强制取消与记录未完成数。
- API 扩散：初版以最小表面为主，复杂扩展通过 RFC 迭代。

---

## ⏱️ 估时（粗略）
- 设计与对齐：0.5d
- 实现（核心循环/trait/迁移）：1.5-2d
- 示例与测试：1-1.5d
- 文档与门禁：0.5d
- 合计：~3.5-4.5d

---

## 📦 交付物
- 代码：`service::{net_server.rs, connection_service.rs, listener.rs}` 等
- API：`NetServer`、`ConnectionService`、`serve_with_connection_handler`
- 示例：`examples/net_server_basic/`、`examples/net_server_custom_protocol/`
- 文档：rustdoc、RFC 状态更新、README 特性列表
- 测试：单测 + 集成测试

---

## 🔚 完成定义（DoD）
- 所有验收标准达成；CI 通过；示例可运行。
- 文档与迁移说明（如必要）同步。
- 与 `PLAN.md` 保持一致（Phase 1 / v2.11）。
