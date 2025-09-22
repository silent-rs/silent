# Silent 异步运行时解耦与兼容方案（需求整理）

## 背景
当前框架在若干模块（如网络、定时器、任务调度）上直接依赖 Tokio，
导致在非 Tokio 场景下（如 async-std、smol、基于 io-uring 的 runtime）难以集成。
本需求旨在在不牺牲现有功能与性能的前提下，完成运行时解耦，
使 Silent 能在多种异步运行时上运行或通过后端适配运行。

## 目标
- 脱离对 Tokio 的强绑定，保持对运行时的中立性。
- 在不破坏核心 API 的前提下，隐藏具体 runtime 类型，稳定公共接口。
- 不再新增 feature 控制运行时选择，由“用户所使用/启动的运行时”在执行时确定。
- 对关键能力（网络、定时器、任务调度、信号、通道、同步）给出统一抽象。

## 不在本次范围
- 不引入 nightly/不稳定特性。
- 不强制改写现有业务层代码（路由/中间件 API 尽量保持不变）。
- 不一次性切换所有示例到新后端（逐步迁移与验证）。

## 可用异步运行时清单（Rust）
以下为当前生态中主流或有代表性的异步运行时（含 I/O 与调度器）：

1. Tokio
   - 生态最广、功能最全（网络、定时器、同步原语、信号、fs 等）。
   - 广泛被 Hyper、Tonic、Reqwest 等库采用。

2. async-std
   - API 风格与标准库一致，提供全套 async I/O、定时器与任务调度。
   - 搭配 async-global-executor/async-io 使用较多。

3. Smol（及其家族：async-executor、async-io、async-net）
   - 轻量、可嵌入；组件化强，可与多种执行器组合。
   - `async-net`/`async-io` 提供跨运行时的网络与 I/O 能力。

4. Monoio（基于 io_uring，Linux）
   - 关注极致性能的 Linux 专用运行时，API 接口逐步完善。
   - 对网络 I/O 友好，但生态适配需要单独适配层。

5. Glommio（thread-per-core，Linux）
   - 每核线程模型，适合高吞吐场景；需要 Linux 环境与特定 CPU 亲和设置。
   - 生态适配相对较少，适合作为实验性后端。

说明：
- Actix-rt 等多为 Tokio 包装或特定框架内置执行器，不单列为独立通用 runtime。
- Embassy 偏向嵌入式异步，暂不作为通用服务器场景优先目标。

## 能力矩阵（关注统一抽象）
- 任务调度：`spawn`、`spawn_blocking`、任务取消/JoinHandle。
- 定时器：`sleep`、`interval`、`timeout`。
- 网络 I/O：`TcpListener/TcpStream`、`UdpSocket`、`Unix*`（可选）。
- 文件系统：异步文件读写（可按需后置）。
- 通道与同步：`mpsc/oneshot`、`Mutex/RwLock/Semaphore`（优先 `futures` 抽象）。
- 信号处理：`ctrl_c` 等基本信号（平台相关）。
- TLS：结合 `rustls`，通过 tokio/async-io 适配。

## 设计原则与实现策略
1. 单一路径“运行时中立”实现
   - 框架内部不再以 feature 分叉运行时后端，只提供一套中立抽象：
     - 任务调度：定义最小 `Spawner` trait（如 `spawn(fut)`、`spawn_blocking`）。
     - 定时器：以 `async_io::Timer` 或 `futures_timer` 作为实现。
     - 网络 I/O：基于 `async-net`/`async-io` 实现 `TcpListener/TcpStream` 等。
   - 公共 API 不暴露特定运行时类型，使用自定义 newtype/trait 屏蔽。

2. 执行时由用户所处运行时决定
   - 框架不主动创建/固定运行时，交由用户以其选择的执行器运行：
     - Tokio：用户 `#[tokio::main]`，通过 `async_compat` 适配 `async-io`/`async-net`。
     - async-std：用户 `#[async_std::main]`，原生兼容 `async-io` 路径。
     - smol：用户使用 `smol::block_on` 或 `async_global_executor` 启动。
   - 对于需要 `spawn` 的场景：
     - 通过依赖注入提供 `Spawner`（在构建 `App`/`Server` 时传入），
       若未提供则回退到 `async_global_executor::spawn`。

3. 通道与同步原语
   - 优先使用 `futures` 提供的 `channel` 与 `lock`（或最小自封装），
     避免直接暴露 `tokio::sync` 类型到公共 API。

4. HTTP/服务器后端策略
   - 保留现有 HTTP 语义，重写/抽象传输层为基于 `async-io` 的实现；
   - 在 Tokio 环境下通过 `async_compat` 运行，无需新增 feature。

5. 适配与示例（无 feature 分叉）
   - 示例分别给出三种运行方式：Tokio、async-std、smol；
   - 文档说明何时需要引入 `async-compat` 以及最小用法。

## 迁移步骤（里程碑）
M0 文档与设计：
- 完成本需求文档与能力盘点（当前阶段）。

M1 最小抽象落地：
- 抽取 `Spawner`/`Timer`/`Net` 最小 trait，并以 `async-io`/`async-net` 实现；
- 框架内部移除对 `tokio::*` 公开类型的依赖；
- 确保 `cargo check --all` 通过，现有示例在“Tokio + async_compat”与 async-std 下可运行。

M2 示例与验证：
- 提供三个最小示例：Tokio（含 `async_compat`）、async-std、smol；
- 在 CI 中增加三种运行方式的 `cargo check`/`clippy`。

M3 覆盖面扩展：
- 路由/中间件/会话等模块移除对特定 runtime 类型的直接暴露；
- 维护兼容层，保证公共 API 稳定。

M4 性能与基准：
- 对比 tokio 与通用后端在若干典型场景下的性能差异，
  评估是否需要为某些热路径保留专用实现。

## 兼容性与潜在破坏性变更
- 移除/隐藏 `tokio::*` 类型出现在公共 API（如返回值/参数）中的情况；
- 不新增 feature 开关，所有运行方式走统一代码路径；
- 如果必须变更公开类型，遵循语义化版本，提供迁移说明与临时适配层。

## 验收标准（最小可用）
- 默认特性（Tokio）下：现有示例编译与运行不回退。
- 启用 `runtime-async-std` 或 `runtime-smol` + `net-async-io`：
  - 能启动最小 HTTP 服务，完成基本路由与中间件链的处理；
  - 定时、spawn、通道等基础能力可用。

## 风险与对策
- HTTP 层强绑定 Hyper/Tokio：
  - 对策：分层后端，保留 tokio 路径，增补基于 async-io 的后端。
- 不同 runtime 的信号/文件系统差异：
  - 对策：将非关键能力后置到后续迭代，优先保证网络与定时器。
- 生态依赖（如依赖库仅支持 Tokio）：
  - 对策：以 feature 可选方式隔离，限定在 tokio 路径中启用。

## 后续工作建议
- 在 `docs/` 增加运行时选择与示例说明文档（含 Tokio + async_compat 指南）。
- 在基准目录新增不同运行方式的压测脚本与报告（可选）。

## 运行示例（草案）
- Tokio 环境：
  ```rust
  // Cargo.toml 需要：tokio, async-compat
  #[tokio::main]
  async fn main() {
      let app = silent::App::new();
      // 可选：提供 Spawner 注入，否则使用 async_global_executor 兜底
      // app.with_spawner(tokio_spawner());
      async_compat::Compat::new(async move {
          silent::serve(app).await
      }).await;
  }
  ```

- async-std 环境：
  ```rust
  #[async_std::main]
  async fn main() {
      let app = silent::App::new();
      silent::serve(app).await;
  }
  ```

- smol 环境：
  ```rust
  fn main() {
      smol::block_on(async {
          let app = silent::App::new();
          silent::serve(app).await;
      });
  }
  ```
