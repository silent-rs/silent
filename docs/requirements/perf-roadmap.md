# Silent 性能优先路线与落地清单（2025-08-18）

本文档整理 Silent 在“高性能优先”定位下的技术路线、阶段目标、PR 粒度与验收标准，覆盖短中长期。所有任务在不破坏对外 API 的前提下优先推进，必要变更将通过明确的迁移指南给出。

## 目标与度量
- 性能目标（短期）：与 Axum/Actix 在同机/同编译参数下 A/B 场景差距 ≤ 5%
- 基准场景：
  - A：GET /ping 返回固定 12B 文本
  - B：解析 3 个路径参数 + 5 个查询参数 + 回 JSON（无 DB）
  - C：1KiB 静态文件（含 ETag/If-None-Match）
- 指标：吞吐（RPS）、p50/p90/p99 延迟、CPU 利用率、内存占用
- 工具：bombardier/wrk + pprof-rs（flamegraph 工件）
- 对标框架：Axum、Actix（提供复现脚本与固定编译参数）

## 一、立刻可做（1–2 周）
1) Hyper v1 全家桶与 http-body 1.x 对齐（PR1）
- 依赖对齐：hyper 1.x、http 1.x、http-body 1.x、hyper-util
- 响应体实现统一切到 `Body` trait
- 全面采用 `bytes::Bytes` 作为 body/切片载体
- 影响面：最小；提供 Changelog 与迁移提示

2) 零拷贝与小对象优化（随 PR1/PR2 渐进落地）
- Header 与字符串：尽量保留借用（`HeaderValue`/`Cow<'_, str>`）
- 热路径：`SmallVec`、`#[inline]`、栈上放置小枚举与 `Option`
- 路由参数仅切片借用，不 `String` 分配

3) TCP/监听优化（PR4）
- `socket2` 配置：`SO_REUSEADDR`、`TCP_NODELAY`、`SO_KEEPALIVE`
- Linux 可选 `SO_REUSEPORT`（文档注明平台差异）

4) Tokio 运行时与阻塞隔离（PR8）
- 默认多线程 runtime
- 提供 `blocking` 特性：将阻塞/CPU 密集任务交由专用线程池（如 rayon）

5) Router 微优化（PR2）
- 内核改造为 Radix/Trie 或 `matchit` 风格：先方法、再静态段、再参数段
- 启动期预编译路由表；运行期仅做切片/整数比较与少量分支

6) 中间件栈零开销（PR3）
- 泛型静态分发（Tower Layer 风格），移除热路径 `Box<dyn Service>`
- 提供可选 Boxed 逃生阀，默认禁用

7) 基准测试基线（PR5）
- 在 `benchmark/` 提供 A/B/C 场景可复现工程与脚本
- CI 产出 p50/p90/p99、RPS、CPU、flamegraph 工件；默认对比 Axum 与 Actix

8) 提前验证：`serve_per_core()` 预览（第一批次，Linux 优先）
- 提供可选入口以在 Linux 上启用 `SO_REUSEPORT` + 每核 runtime
- 标注为实验特性（feature flag），用于尽早验证端到端收益

## 二、中期架构演进（1–2 个月）
1) 每核一 runtime/进程（新增 `serve_per_core()`）（已提前到第一批次的实验特性）
- 利用 `SO_REUSEPORT`（Linux）分散同端口连接，各自 pin 到 CPU 核
- 降低跨核抢占与工作窃取开销；保留默认 `serve()` 兼容模式

2) HTTP/2 打磨与 HTTP/3 预研（PR6 + 预研分支）
- 暴露 H2 流控参数（窗口/并发/头压缩）
- `Content-Length`/`transfer-encoding` fast-path
- H3 通过 quinn/h3 预研，不默认启用

3) 静态文件与缓存层（PR7）
- 预压缩（brotli/gzip/zstd）与条件请求（ETag/If-Modified-Since）
- 大文件分块 `Bytes` 流式；目录索引、范围请求零分配热路径

4) 解析器零拷贝（中期）
- JSON：默认 `serde_json`，可选 `simd-json` 特性（x86_64 优化）
- 表单/URL：借用原始缓冲区；值用切片/索引复用
- Multipart：背压感知解析器，避免大上传占满内存

5) WebSocket/SSE 吞吐优化（中期）
- WebSocket 可选 fastwebsockets 或保留实现并加入零拷贝帧缓冲池
- SSE 合并小包、批量 flush 降低 syscalls

6) 观测性与调参闭环（PR8 扩展）
- 集成 `tracing` + OTLP exporter；延迟直方图与热点 span
- 暴露运行时指标（活动连接、拥塞写、slow_request 计数）
- 提供 Prometheus+Grafana 仪表盘示例（docker-compose）

## 三、长期差异化（>3 个月）
1) 无宏·零样板的编译期路由器
- const fn + 可选 proc-macro 生成；运行时回退路径保底

2) 智能网络自适应
- 基于 RTT/丢包自调写入批量与 Nagle 策略
- 高并发下延迟/吞吐模式自动切换

3) 场景化优化包
- API 网关：Rate-limit、限流队列、公平调度、熔断/隔离（静态分发）
- 全静态/边缘：Aggressive 缓存、预拉取、ETag 批量校验
- 低延迟交易：线程 pin、CPU/NUMA 亲和、锁规避模板

## 对齐现代框架的补齐项
- 薄封装 + 静态分发：Tower-Layer 风格泛型栈，避免动态派发
- Hyper v1 生态：`http-body`/`hyper-util`/H2 参数暴露；H3 规划
- IO & 运行时拓扑：`SO_REUSEPORT` + 每核 runtime 一等支持
- 零拷贝与内存纪律：`Bytes` 全面化、提取器借用化、`SmallVec`/`#[inline]`
- 基准与可观测：可复现基准工程 + 指标/火焰图链路
- 文档与样例：README 明确性能模式与复现步骤，维护与 Axum 的公开对比基准链接

## PR 粒度与依赖关系
- PR1（高优先）：Hyper v1 & http-body 1.x 迁移、`Bytes` 贯通
- PR3（高优先）：中间件栈静态分发（移除热路径 Box<dyn>）
- PR5（高优先）：基准工程 + CI（A/B/C + 指标 + flamegraph；加入 Axum/Actix 对标）
- PR4（高优先）：socket2 监听与 TCP 参数、可选 `SO_REUSEPORT`
- PR2（高优先）：Router 内核改造（Trie + 借用提取）
- PR6（中优先）：H2 参数暴露与 fast-path（固定 Content-Length）
- PR7（中优先）：静态文件与缓存（ETag/Range/预压缩）
- PR8（中优先）：阻塞隔离（rayon）与观测面板（tracing/metrics）

第一批次（优先启动，部分可并行）：PR1、PR5、PR3；`serve_per_core()` 以实验特性并入 PR4 或单独小 PR（Linux）

依赖建议：
- PR1 先行，为 PR3/PR2 提供统一底座
- PR5 与 PR1 可并行推进（基准作为回归防线）
- `serve_per_core()`（Linux）可并入 PR4 并提早验证端到端收益

## 验收标准（每 PR 均需）
- 编译通过、`cargo check`/`cargo clippy` 零阻塞问题
- 基准在相同硬件/参数下重复 3 次，方差在可接受范围内
- A/B 场景与 Axum 对比差距 ≤ 5%（阶段性目标）
- 文档：更新 README 与 `docs/` 相关章节，给出复现步骤
- 观测：关键路径提供 `tracing` span（不开销热路径）

## 参考与实现注意
- 保持“少/无宏”哲学；仅在编译期路由器作为可选 proc-macro
- 对外 API 尽可能稳定；必要变更提供迁移指南
- `ID` 均使用 `scru128`；时间字段使用 `chrono::Local::now().naive_local()`
- 前端样例/工具链使用 `yarn`
- 代码检查：Rust 优先 `cargo check`/`cargo clippy`；前端优先 build
