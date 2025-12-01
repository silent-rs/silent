# Silent 框架项目规划

## 愿景与目标

### 核心目标
- **性能卓越**：进入 Rust Web 框架性能 Top 5，缩小与 Actix-web/Axum 的差距
- **生产可用**：提升 server 模块生产级能力，统一配置入口、连接保护、公平监听、可观测性
- **QUIC 领先**：在 QUIC/HTTP3/WebTransport 场景形成性能优势
- **易于运维**：提供可运维化的默认配置与可观察行为，降低生产部署风险

### 量化指标
- TechEmpower Benchmark 排名进入 Rust 框架 Top 5
- 延迟 P99 相比当前降低 30-40%
- 吞吐量提升 30-50%
- 与 Axum 性能差距缩小到 10% 以内

---

## 版本里程碑

### v2.12 - 性能优化基础（当前版本）
**主题**：零成本抽象改造 + 快速收益优化
**时间**：2025-12-01 ~ 2025-12-31
**核心改进**：
- 移除 async_trait，采用 RPITIT (Return Position Impl Trait In Traits)
- 优化 HyperServiceHandler，消除 Box::pin 堆分配
- 减少热路径克隆操作
- 编译器优化配置（LTO、PGO）

**预期收益**：延迟降低 15-25%，吞吐提升 15-20%

### v2.13 - 路由与内存优化
**主题**：路由算法重构 + 内存效率提升
**时间**：2026-01-01 ~ 2026-02-15
**核心改进**：
- **M1**：基数树 (Radix Tree) 路由实现
  - 前缀压缩减少节点数量
  - 预计算静态路径哈希
  - 延迟参数解析优化
- **M2**：零拷贝参数提取
  - 使用切片引用避免字符串分配
  - Cow<'_, str> 优化
- **M3**：对象池复用机制
  - Request/Response 对象池
  - SmallVec 容量调优

**预期收益**：累计延迟降低 30-40%，吞吐提升 30-40%

### v2.14 - Server 生产化与可观测性
**主题**：统一配置 + 连接保护 + 监听公平 + 可观测性
**时间**：2026-02-15 ~ 2026-04-01
**核心改进**：
- **M1**：统一配置入口 + 连接保护
  - 集中 server 配置（TCP/TLS/QUIC）
  - per-connection 超时控制
  - 请求体大小限制
  - 限流默认值配置
- **M2**：监听公平性与错误退避
  - 多监听器公平 accept
  - 错误退避机制
  - 健康检测
- **M3**：可观测性与 QUIC 生产化
  - metrics/tracing 埋点
  - Alt-Svc/ALPN 对齐
  - QUIC 参数优化
  - WebTransport 限制

**预期收益**：生产稳定性提升，运维可观测性完善

### v3.0 - 生态与特性完善
**主题**：框架成熟化 + 生态建设
**时间**：2026-04-01 之后
**核心方向**：
- 完善文档和示例
- 扩展中间件生态
- 集成常用工具链（OpenAPI、模板引擎等）
- 社区建设与推广

---

## 当前阶段优先级（v2.12）

### P0 - 立即开始（最高优先级）
**目标**：快速收益，建立性能优化基础

1. **移除 async_trait**
   - 影响：handler、middleware、所有 trait 定义
   - 收益：10-15% 延迟降低
   - 风险：中等（需要大量代码改动）
   - 时间：2 周

2. **消除 Box::pin 堆分配**
   - 位置：`hyper_service.rs:58`、所有 Future 返回值
   - 收益：3-5% 延迟降低
   - 风险：低
   - 时间：3 天

3. **减少克隆操作审计**
   - 目标：移除不必要的 `.clone()` 调用
   - 收益：5-8% CPU 使用率降低
   - 风险：低
   - 时间：1 周

### P1 - 并行进行（高优先级）

4. **泛型化 Handler 系统**
   - 替代 `Arc<dyn Handler>` 为泛型
   - 收益：5-10% 吞吐提升
   - 风险：中等（二进制大小增加）
   - 时间：2 周

5. **路由树优化 POC**
   - 基数树算法验证
   - 基准测试对比
   - 收益评估
   - 时间：1 周

### P2 - 后续优化（中优先级）

6. **编译器优化配置**
   - LTO、PGO、target-cpu=native
   - 收益：3-5%
   - 时间：2 天

7. **性能监控体系**
   - CI 集成 criterion benchmark
   - 性能回归检测
   - 火焰图分析
   - 时间：3 天

---

## 详细范围与验收要点

### 性能优化（v2.12-v2.13）

#### 零成本抽象改造
**范围**：
- Handler trait：移除 async_trait，使用 RPITIT 或手动实现
- MiddleWareHandler trait：同上
- 所有 trait 实现的调整

**验收**：
- 编译通过，所有测试通过
- criterion benchmark 显示延迟降低 ≥10%
- 火焰图显示堆分配减少 ≥50%

#### 路由优化
**范围**：
- 实现基数树路由（参考 matchit crate）
- 零拷贝参数提取
- 路由表预编译

**验收**：
- 路由匹配性能提升 ≥10%（深层嵌套场景）
- 内存分配减少 ≥30%
- 向后兼容现有 API

#### 内存优化
**范围**：
- Request/Response 对象池
- SmallVec 容量调优
- Cow<'_, str> 使用

**验收**：
- 堆分配次数减少 ≥20%
- 内存峰值降低 ≥15%
- 无内存泄漏

### Server 生产化（v2.14）

#### 统一配置入口
**范围**：
- 集中配置结构（TCP/TLS/QUIC）
- 默认值与覆盖策略
- Builder 模式 + serde 支持

**验收**：
- 配置 API 统一清晰
- 向后兼容
- 文档完善

#### 连接保护
**范围**：
- per-connection 读/写/总超时
- 请求体大小限制
- HTTP/1.1、HTTP/2、HTTP/3 一致行为
- 限流默认值可配置

**验收**：
- 超时机制正确触发
- 大请求被正确拒绝
- 限流器稳定工作

#### 监听公平与退避
**范围**：
- 监听器独立 accept 任务或公平调度
- 连续错误退避（指数/限幅）
- accept 错误可观测

**验收**：
- 多监听器场景下连接均衡
- 错误退避正确工作
- 日志和指标完整

#### 可观测性
**范围**：
- tracing span 携带 peer/listener 信息
- 指标覆盖：accept 成功/失败、限流命中、请求耗时、关停耗时
- QUIC/WebTransport 会话与帧计数

**验收**：
- tracing 输出结构化清晰
- metrics 导出完整
- 支持 OpenTelemetry

#### QUIC 生产化
**范围**：
- quinn transport 参数优化（idle_timeout、max_streams、窗口）
- WebTransport/HTTP3 限制
- Alt-Svc 自动端口匹配
- ALPN 配置

**验收**：
- QUIC 连接稳定
- 参数可配置
- Alt-Svc 正确工作

---

## 技术选型与实现方向

### 性能优化技术栈

#### 异步抽象
```rust
// 方案 1：RPITIT (Rust 1.75+)
pub trait Handler: Send + Sync + 'static {
    fn call(&self, req: Request) -> impl Future<Output = Result<Response>> + Send;
}

// 方案 2：手动实现（兼容性更好）
pub trait Handler: Send + Sync + 'static {
    type Future: Future<Output = Result<Response>> + Send;
    fn call(&self, req: Request) -> Self::Future;
}
```

#### 路由算法
- **基数树**：参考 matchit/route-recognizer
- **前缀压缩**：减少节点数量
- **完美哈希**：静态路由优化
- **零拷贝**：参数提取使用切片引用

#### 内存管理
- **对象池**：lockless 实现（crossbeam）
- **SmallVec**：根据统计调优容量
- **Cow**：按需分配

#### 编译优化
```toml
[profile.release]
codegen-units = 1
lto = "fat"
opt-level = 3
strip = true
```

```bash
# PGO 两步构建
RUSTFLAGS="-C profile-generate=/tmp/pgo" cargo build --release
# 运行 benchmark 收集数据
RUSTFLAGS="-C profile-use=/tmp/pgo" cargo build --release
```

### Server 生产化技术栈

#### 配置管理
- **集中配置**：ServerConfig 结构统一入口
- **默认值**：合理的生产默认配置
- **覆盖策略**：Builder 模式 + serde 反序列化

#### 超时与限制
- **Tokio 超时**：`tokio::time::timeout` 包装
- **Body 限流**：HTTP/1.1、HTTP/2 使用 body 限流
- **HTTP/3 限制**：流式聚合限制

#### 监听公平性
- **独立任务**：每个监听器独立 accept 任务
- **select/JoinSet**：公平调度
- **错误退避**：`tokio::time::sleep` + 指数退避限幅

#### 可观测性
- **tracing**：span/field 结构化日志
- **metrics**：兼容 metrics/otel 接口
- **无具体后端依赖**：用户可选择后端

#### QUIC 实现
- **quinn + rustls**：成熟稳定
- **ALPN**：支持 h3/h3-29
- **Alt-Svc**：端口自动对齐
- **backpressure**：WebTransport/Datagram 限制

---

## 关键时间节点

### v2.12 性能优化基础（2025-12）
- **2025-12-01 ~ 2025-12-07**：移除 async_trait（P0）
- **2025-12-08 ~ 2025-12-14**：消除 Box::pin + 克隆优化（P0）
- **2025-12-15 ~ 2025-12-21**：泛型化 Handler 系统（P1）
- **2025-12-22 ~ 2025-12-28**：路由树优化 POC（P1）
- **2025-12-29 ~ 2025-12-31**：编译优化 + 性能验证（P2）

**里程碑验收**（2025-12-31）：
- 延迟降低 ≥15%
- 吞吐提升 ≥15%
- 所有测试通过

### v2.13 路由与内存优化（2026-01 ~ 2026-02）
- **2026-01-01 ~ 2026-01-21**：基数树路由实现（M1）
- **2026-01-22 ~ 2026-02-04**：零拷贝参数提取（M2）
- **2026-02-05 ~ 2026-02-15**：对象池复用机制（M3）

**里程碑验收**（2026-02-15）：
- 累计延迟降低 ≥30%
- 累计吞吐提升 ≥30%
- 进入 Rust 框架 Top 5

### v2.14 Server 生产化（2026-02 ~ 2026-04）
- **2026-02-15 ~ 2026-03-07**：统一配置 + 连接保护（M1）
- **2026-03-08 ~ 2026-03-21**：监听公平 + 错误退避（M2）
- **2026-03-22 ~ 2026-04-01**：可观测性 + QUIC 生产化（M3）

**里程碑验收**（2026-04-01）：
- 生产环境稳定运行
- 可观测性完善
- QUIC 性能领先

---

## 风险管理

### 高风险项

#### 1. RPITIT 迁移（v2.12）
**风险**：破坏 API 兼容性，大量代码改动
**影响**：延期 1-2 周
**缓解措施**：
- 分阶段迁移，保留旧 API 作为过渡
- 完善测试覆盖率
- 提供迁移指南

#### 2. 路由算法重构（v2.13）
**风险**：功能回归，边界情况处理不当
**影响**：性能提升不达预期
**缓解措施**：
- 保留旧实现作为对照
- 完善单元测试和集成测试
- 基准测试持续验证

### 中风险项

#### 3. 泛型化膨胀（v2.12）
**风险**：编译时间和二进制大小显著增加
**影响**：开发体验下降
**缓解措施**：
- 使用 feature flag 控制
- 增量式泛型化
- 监控编译时间和二进制大小

#### 4. 对象池并发竞争（v2.13）
**风险**：并发场景下性能反而下降
**影响**：优化收益降低
**缓解措施**：
- 使用无锁数据结构（crossbeam）
- 充分的并发测试
- 提供开关配置

### 低风险项
- 编译器优化配置
- 日志和 tracing 优化
- 内联提示添加

---

## 成功标准

### v2.12 验收标准
- [ ] 所有 91 处 Box::pin 减少到 ≤10 处
- [ ] criterion benchmark 延迟降低 ≥15%
- [ ] wrk 测试吞吐提升 ≥15%
- [ ] 所有单元测试和集成测试通过
- [ ] 火焰图显示堆分配减少 ≥50%
- [ ] 编译无 warning

### v2.13 验收标准
- [ ] 路由匹配性能提升 ≥10%（深层嵌套）
- [ ] 累计延迟降低 ≥30%
- [ ] 累计吞吐提升 ≥30%
- [ ] 内存分配减少 ≥30%
- [ ] TechEmpower Benchmark 进入 Rust Top 5
- [ ] API 向后兼容

### v2.14 验收标准
- [ ] 配置系统统一清晰
- [ ] 超时和限流机制正确工作
- [ ] 多监听器公平调度验证通过
- [ ] tracing 和 metrics 输出完整
- [ ] QUIC 生产环境稳定运行
- [ ] 文档完善

---

## 性能测试基准

### 测试环境
**硬件**：
- CPU: 8 核以上
- 内存: 16GB+
- 网络: 千兆网卡

**软件**：
- Rust: 最新 stable
- Tokio: 最新版本
- 对比框架：Actix-web、Axum、Hyper 最新稳定版

### 测试场景
1. **TechEmpower A**：GET / 返回 12B 文本
2. **TechEmpower B**：解析路径参数 + 查询参数 + JSON 响应
3. **TechEmpower C**：1KiB 静态文件 + ETag
4. **深层路由**：10 层嵌套路由匹配
5. **高并发**：1000+ 并发连接

### 测试工具
- wrk / wrk2：HTTP 压测
- criterion：Rust benchmark
- cargo-flamegraph：火焰图分析
- perf / Instruments：性能剖析
- valgrind：内存分析

---

## 参考资料

### 性能优化
- [Rust async trait 性能问题](https://github.com/dtolnay/async-trait/issues/1)
- [matchit - 快速路由匹配](https://github.com/ibraheemdev/matchit)
- [PGO in Rust](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
- [Rust Web Frameworks Compared: Actix vs Axum vs Rocket](https://dev.to/leapcell/rust-web-frameworks-compared-actix-vs-axum-vs-rocket-4bad)
- [Axum vs Actix: Rust Web 框架比较](https://zhuanlan.zhihu.com/p/713552734)

### Server 生产化
- [Hyper 1.0 性能优化](https://hyper.rs/)
- [Tokio 最佳实践](https://tokio.rs/tokio/tutorial)
- [Quinn QUIC 实现](https://github.com/quinn-rs/quinn)
- [OpenTelemetry Rust](https://github.com/open-telemetry/opentelemetry-rust)

### 基准测试
- [TechEmpower Benchmarks](https://www.techempower.com/benchmarks/)
- [Web Frameworks Benchmark](https://web-frameworks-benchmark.netlify.app/)

---

## 附录：组织与协作

### 责任分工
- **核心开发**：性能优化、路由重构
- **测试验证**：benchmark、压测、回归测试
- **文档维护**：API 文档、迁移指南、性能报告
- **代码审查**：确保代码质量和性能目标

### 沟通机制
- **周报**：每周五发布进度和性能数据
- **里程碑评审**：每个 M 完成后进行评审
- **紧急问题**：性能回归或严重 bug 立即处理

### 质量保障
- **Code Review**：所有 PR 必须经过审查
- **CI/CD**：自动化测试和性能基准
- **性能监控**：每次 commit 运行 benchmark
- **回归检测**：性能下降 >5% 触发警告
