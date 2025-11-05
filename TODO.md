# TODO（quichttp3 稳定化专项）

> 分支: `feature/quichttp3-stabilization`（自 `main` 切出）
> 目标版本: v2.12（阶段性稳定）

## 背景与目标
- 稳定 QUIC/HTTP/3（h3）在服务端的请求-响应通路与 WebTransport 握手行为；
- 保持对外 API 不变，内部以最小抽象保障可测性与可维护性；
- 通过覆盖率与 clippy 门禁确保质量。

## 当前覆盖率基线（以 2025-11-05 为基准）
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

## 验收标准（Definition of Done）
- 质量门禁：`cargo fmt`、`cargo clippy --all-targets --all-features --tests --benches -- -D warnings`、`cargo check` 通过；
- 测试：`cargo test -p silent --all-features` 全量通过；
- 覆盖率提升目标：
  - `connection.rs`: 行覆盖率从 7.69% → 60%+
  - `listener.rs`: 行覆盖率从 3.23% → 60%+
  - `echo.rs`: 行覆盖率从 0% → 80%+
  - `service.rs`: 函数覆盖率从 50% → 70%+
  - 总体目标：server/quic 模块行覆盖率 60%+；
- 文档：更新 `docs/quic-webtransport.md` 与 `PLAN.md` 相关条目；
- 无对外 API 破坏性变更。

## 任务拆解（单一职责，可测试，标注依赖）

### ✅ 1) 抽象与命名收敛（已完成）
- `H3RequestIo` trait 已实现（最小方法集：`recv_data`/`send_response`/`send_data`/`finish`）；
- `build_webtransport_handshake_response()` 已提取为独立函数；
- 验证：`cargo check`、相关单测通过（4/4 通过）。

### ⚠️  2) HTTP/3 请求-响应路径用例补强（需补强）
**已完成场景：**
- 多帧请求体聚合与回显 ✅
- 空请求体路径 ✅
- 发送响应头失败时错误传播 ✅

**待补强场景：**
- **响应体发送失败时的错误处理**：目前缺少 `send_data()` 失败的测试用例；
- **Body 解析错误路径**：请求体为无效 UTF-8 时的处理；
- **大请求体压力测试**：验证内存使用和流控（可在单元测试中模拟）；
- 依赖：1)；
- 验证：单测新增 3+ 用例，服务.rs 函数覆盖率提升至 70%+。

### ⚠️  3) WebTransport 回显处理器稳定性（核心缺陷 - 0% 覆盖）
**问题**：`server/quic/echo.rs` 完全未测试，存在以下风险点：
- **空消息处理**：客户端发送空数据时的行为；
- **二进制数据处理**：非 UTF-8 数据的回显逻辑；
- **多块数据聚合**：多个 recv_data() 调用的聚合逻辑；
- **流错误传播**：recv_data/send_data/finish 失败的错误处理；
- **会话信息记录**：日志输出的正确性；
- 依赖：1)；
- **目标**：新增 5+ 测试用例，覆盖 `echo.rs` 行覆盖率 80%+。

### ⚠️  4) QUIC 连接类型与协议适配（低覆盖 - 7.69%）
**问题**：`server/quic/connection.rs` 仅验证类型存在，无实际逻辑测试：
- **AsyncRead/AsyncWrite 错误路径**：验证不支持操作的错误信息；
- **into_incoming() 转换**：确保 QUIC 连接正确转换为 quinn::Incoming；
- **类型安全**：确保 QuicConnection 不能被误用作普通流；
- 依赖：无；
- **目标**：新增 3+ 测试用例，connection.rs 行覆盖率提升至 60%+。

### ⚠️  5) QUIC 监听器关闭与竞态路径（极低覆盖 - 3.23%）
**问题**：`server/quic/listener.rs` 几乎未测试，缺少关键路径覆盖：
- **None 返回路径**：`endpoint.accept()` 返回 None 时（监听器关闭）的处理；
- **HybridListener 竞态**：`tokio::select!` 下 QUIC 和 HTTP 监听的竞态条件；
- **错误传播**：bind 失败、local_addr 错误的处理；
- **TLS 配置验证**：证书配置对 QUIC 协议（h3、h3-29）的支持；
- 依赖：无；
- **目标**：新增 4+ 测试用例，listener.rs 行覆盖率提升至 60%+。

### ✅ 6) WebTransport 握手稳定（已完成）
- 提取 `build_webtransport_handshake_response()` ✅
- 测试带/不带 `sec-webtransport-http3-draft` 头部 ✅
- 错误日志信息已包含必要 context ✅
- 验证：单测通过（1/1 通过）。

### 📊 7) 覆盖率基线与目标
- **基线已记录**：见文件顶部基线数据；
- **目标提升**：
  - 短期目标：各模块最低行覆盖率 60%；
  - 长期目标：server/quic 总体行覆盖率 65%+；
- 命令：`cargo llvm-cov nextest --all-features -p silent`；
- 依赖：2)、3)、4)、5)。

### 📝 8) 文档与示例
- `docs/quic-webtransport.md`：补充"测试策略与稳定性"小节：
  - 伪造 H3 流测试策略说明；
  - WebTransport 回显处理器使用示例；
  - 常见错误场景与排查指南；
- `PLAN.md`：保持与分支目标同步；
- 依赖：2)、3)、4)、5)；
- 验证：文档编译通过，无断链。

### 🔒 9) 门禁与 CI
- 本地钩子/CI 统一以 clippy `-D warnings`、deny、nextest 执行；
- nextest 配置：`.config/nextest.toml` 已知 `run-threads` 会被忽略，不影响执行；
- 依赖：全部任务；
- 验证：PR 检查通过。

## 风险评估

### 高风险（需优先处理）
1. **`echo.rs` 0% 覆盖率**：WebTransport 回显功能完全未验证，生产环境可能出现未捕获错误；
2. **`listener.rs` 3% 覆盖率**：监听器关闭路径未测试，可能导致服务器无法优雅关闭；
3. **`connection.rs` 7% 覆盖率**：协议适配层缺乏验证，可能存在类型安全问题。

### 中风险
1. **service.rs 错误路径**：响应体发送失败、body 解析错误等场景覆盖不足；
2. **WebTransport 流错误传播**：缺少 recv_data/send_data/finish 失败的测试。

## 实施优先级
1. **P0**：补充 echo.rs 测试（5+ 用例）→ 提升 WebTransport 功能稳定性；
2. **P0**：补充 listener.rs 测试（4+ 用例）→ 确保监听器关闭路径稳定；
3. **P1**：补充 connection.rs 测试（3+ 用例）→ 验证协议适配；
4. **P1**：补强 service.rs 错误路径（3+ 用例）→ 提升健壮性；
5. **P2**：更新文档与示例；
6. **P2**：验证总体覆盖率达标。

## 备注
- **不进行真实 QUIC/H3 握手集成测试**：保持单测可控与确定性；
- **测试策略**：继续使用 `FakeH3Stream` 和 `H3RequestIo` 抽象进行单元测试；
- **依赖管理**：新测试不得引入额外外部依赖，仅使用 `h3`、`quinn` 已有的测试能力。
