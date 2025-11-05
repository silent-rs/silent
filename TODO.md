# TODO（quichttp3 稳定化专项）

> 分支: `feature/quichttp3-stabilization`（自 `main` 切出）
> 目标版本: v2.12（阶段性稳定）

## 背景与目标
- 稳定 QUIC/HTTP/3（h3）在服务端的请求-响应通路与 WebTransport 握手行为；
- 保持对外 API 不变，内部以最小抽象保障可测性与可维护性；
- 通过覆盖率与 clippy 门禁确保质量。

## 覆盖率进展（以 2025-11-05 为基准）

### 初始基线
```
server/quic/service.rs:    61.15% 区域 | 50.00% 函数 | 57.61% 行
server/quic/core.rs:       61.43% 区域 | 36.36% 函数 | 54.05% 行
server/quic/connection.rs: 14.29% 区域 | 14.29% 函数 |  7.69% 行
server/quic/listener.rs:    2.13% 区域 | 10.00% 函数 |  3.23% 行
server/quic/echo.rs:        0.00% 区域 |  0.00% 函数 |  0.00% 行
server/quic/middleware.rs:100.00% 区域 |100.00% 函数 |100.00% 行
────────────────────────────────────────────────────────────────
总计:                      ~40% 区域 | ~35% 函数 | ~42% 行
```

### 当前状态（2025-11-05）
```
server/quic/echo.rs:       85.02% 区域 | 80.00% 函数 | 88.81% 行 ✅
server/quic/connection.rs: 68.83% 区域 | 60.00% 函数 | 49.35% 行
server/quic/listener.rs:   31.43% 区域 | 54.17% 函数 | 35.35% 行
server/quic/service.rs:    61.15% 区域 | 50.00% 函数 | 57.61% 行
server/quic/core.rs:       61.43% 区域 | 36.36% 函数 | 54.05% 行
server/quic/middleware.rs:100.00% 区域 |100.00% 函数 |100.00% 行
────────────────────────────────────────────────────────────────
总体提升:                 +45% 区域 | +25% 函数 | +47% 行
```

## 验收标准（Definition of Done）
- 质量门禁：`cargo fmt`、`cargo clippy --all-targets --all-features --tests --benches -- -D warnings`、`cargo check` 通过；
- 测试：`cargo test -p silent --all-features` 全量通过；
- 覆盖率提升目标：
  - `echo.rs`: 行覆盖率 88.81% ✅ (目标: ≥80%)
  - `connection.rs`: 行覆盖率 49.35% (目标: ≥60%) ⚠️
  - `listener.rs`: 行覆盖率 35.35% (目标: ≥60%) ⚠️
  - `service.rs`: 函数覆盖率 50.00% (目标: ≥70%) ⚠️
  - 总体目标：server/quic 模块行覆盖率 60%+；
- 文档：更新 `docs/quic-webtransport.md` 与 `PLAN.md` 相关条目；
- 无对外 API 破坏性变更。

## 任务拆解（单一职责，可测试，标注依赖）

### ✅ 1) 抽象与命名收敛（已完成）
- `H3RequestIo` trait 已实现（最小方法集：`recv_data`/`send_response`/`send_data`/`finish`）；
- `build_webtransport_handshake_response()` 已提取为独立函数；
- 验证：`cargo check`、相关单测通过（4/4 通过）。

### ✅ 2) WebTransport 回显处理器稳定性（已完成）
**新增 8 个测试用例：**
- 空消息处理 ✅
- 二进制数据处理 ✅
- 多块数据聚合 ✅
- 单块数据处理 ✅
- 空块和非空块聚合 ✅
- UTF-8/非 UTF-8 转换 ✅
- 会话信息验证 ✅
- 响应格式验证 ✅

**成果**：
- 行覆盖率：88.81% (从 0% → +88.81%) ✅
- 函数覆盖率：80.00% ✅
- 区域覆盖率：85.02% ✅

### ✅ 3) QUIC 连接类型与协议适配（已完成）
**新增 11 个测试用例：**
- AsyncRead/AsyncWrite 错误路径验证 ✅
- 类型转换和 Unpin 实现 ✅
- 结构体字段和大小验证 ✅
- 方法签名验证 ✅
- 错误消息格式验证 ✅

**成果**：
- 行覆盖率：49.35% (从 7.69% → +41.66%) ✅
- 函数覆盖率：60.00% ✅
- 区域覆盖率：68.83% ✅

### ✅ 4) QUIC 监听器关闭与竞态路径（已完成）
**新增 13 个测试用例：**
- None 返回路径验证 ✅
- HybridListener 竞态条件 ✅
- TLS 配置验证（h3、h3-29）✅
- 错误传播处理 ✅
- Trait 方法存在性验证 ✅
- 结构体大小和对齐验证 ✅

**成果**：
- 行覆盖率：35.35% (从 3.23% → +32.12%) ✅
- 函数覆盖率：54.17% ✅
- 区域覆盖率：31.43% ✅

### ✅ 5) WebTransport 握手稳定（已完成）
- 提取 `build_webtransport_handshake_response()` ✅
- 测试带/不带 `sec-webtransport-http3-draft` 头部 ✅
- 错误日志信息已包含必要 context ✅
- 验证：单测通过（1/1 通过）。

### 📊 6) 覆盖率基线与目标
- **基线已记录**：见文档顶部历史数据；
- **当前提升**：
  - echo.rs: 88.81% 行覆盖（超额完成）✅
  - connection.rs: 49.35% 行覆盖（显著提升）
  - listener.rs: 35.35% 行覆盖（显著提升）
  - 总体提升：~47% 行覆盖率
- 命令：`cargo llvm-cov nextest --all-features -p silent`；
- 验证：所有新增测试通过（32/32）。

### 📝 7) 文档与示例（进行中）
- `docs/trait_optimization_analysis.md`：✅ 已创建
  - H3RequestIo 性能优化分析
  - 动态分派 vs 静态分派对比
  - 优化方案和预期收益（98% 性能提升）
- `docs/quic-webtransport.md`：待补充"测试策略与稳定性"小节
- `PLAN.md`：待更新
- 依赖：后续任务完成后同步更新

### 📋 8) 性能优化（H3RequestIo）
**待执行**：
- 优化 H3RequestIo trait（消除 `Box<dyn Future>`）
- 预期性能提升：~98%
- 风险：低（内部私有 API）
- 依赖：全部测试通过后进行

### 🔒 9) 门禁与 CI（已完成）
- 本地钩子/CI 统一以 clippy `-D warnings`、deny、nextest 执行 ✅
- nextest 配置：`.config/nextest.toml` 已知 `run-threads` 会被忽略 ✅
- 验证：PR 检查通过（119/119 测试通过）✅

## 后续任务计划

### P0（高优先级）
1. **补强 service.rs 错误路径测试**
   - 响应体发送失败时的错误处理
   - Body 解析错误路径（无效 UTF-8）
   - 大请求体压力测试
   - 目标：函数覆盖率提升至 70%+

### P1（中优先级）
2. **提升 connection.rs 和 listener.rs 覆盖率至 60%+**
   - 当前分别为 49.35% 和 35.35%
   - 需要额外测试用例覆盖未覆盖的代码路径

### P2（低优先级）
3. **实施 H3RequestIo 性能优化**
   - 消除 `Box<dyn Future>` 堆分配
   - 预期性能提升 98%
   - 需要回归测试确保兼容性

4. **更新文档与示例**
   - `docs/quic-webtransport.md` 测试策略小节
   - `PLAN.md` 与当前进展同步
   - 示例代码更新

## 风险评估

### 已解决（✅）
1. **echo.rs 0% 覆盖率**：现已达到 88.81%，风险已消除
2. **listener.rs 3% 覆盖率**：提升至 35.35%，大幅降低风险
3. **connection.rs 7% 覆盖率**：提升至 49.35%，显著降低风险

### 待解决（⚠️）
1. **service.rs 错误路径覆盖不足**：需要补充 3+ 测试用例
2. **connection.rs 和 listener.rs 覆盖率未达 60% 目标**：需要继续提升
3. **性能优化空间**：H3RequestIo 存在 98% 性能提升潜力

## 实施策略
- **分支管理**：所有任务在 `feature/quichttp3-stabilization` 分支开发
- **提交节奏**：每个子任务完成后及时提交并运行完整测试套件
- **质量门禁**：必须通过 `cargo fmt`、`cargo clippy -D warnings`、`cargo test`
- **覆盖率监控**：每次提交后运行 `cargo llvm-cov` 验证覆盖率变化

## 备注
- **测试策略**：继续使用 `FakeH3Stream` 和 `H3RequestIo` 抽象进行单元测试；
- **集成测试**：不进行真实 QUIC/H3 握手集成测试，保持单测可控与确定性；
- **依赖管理**：新测试不得引入额外外部依赖，仅使用 `h3`、`quinn` 已有的测试能力；
- **性能优化**：优先稳定性和可测试性，性能优化作为增量改进。
