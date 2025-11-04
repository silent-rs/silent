# TODO（quichttp3 稳定化专项）

> 分支: `feature/quichttp3-stabilization`（自 `main` 切出）
> 目标版本: v2.12（阶段性稳定）

## 背景与目标
- 稳定 QUIC/HTTP/3（h3）在服务端的请求-响应通路与 WebTransport 握手行为；
- 保持对外 API 不变，内部以最小抽象保障可测性与可维护性；
- 通过覆盖率与 clippy 门禁确保质量。

## 验收标准（Definition of Done）
- 质量门禁：`cargo fmt`、`cargo clippy --all-targets --all-features --tests --benches -- -D warnings`、`cargo check` 通过；
- 测试：`cargo test -p silent --all-features` 全量通过；
- 覆盖：`cargo llvm-cov nextest --all-features -p silent` 可产出报告，server/quic 行/函数覆盖率较当前基线提升；
- 文档：更新 `docs/quic-webtransport.md` 与 `PLAN.md` 相关条目；
- 无对外 API 破坏性变更。

## 任务拆解（单一职责，可测试，标注依赖）

1) 抽象与命名收敛（已完成/持续审视）
- 保持 `silent/src/server/quic/service.rs` 内部私有 `H3RequestIo`（最小方法集：`recv_data`/`send_response`/`send_data`/`finish`）。
- 依赖：无；
- 验证：`cargo check`、相关单测通过。

2) HTTP/3 请求-响应路径用例补强
- 多帧请求体聚合与回显（已覆盖）；
- 空请求体路径（已覆盖）；
- 发送响应头失败时错误传播（已覆盖）；
- 依赖：1)；
- 验证：单测通过。

3) WebTransport 握手稳定
- 提取 `build_webtransport_handshake_response()`（已完成）；
- 测试带/不带 `sec-webtransport-http3-draft` 头部（已覆盖）；
- 错误日志信息审视（必要时补充 `context`）；
- 依赖：1)；
- 验证：单测通过。

4) 监听器关闭/竞态路径稳定性（配合主循环）
- 覆盖 `Listeners::accept()` 竞态与关闭路径（已有多监听快/慢分支覆盖，必要时补齐 `None` 分支）；
- 依赖：无；
- 验证：单测通过。

5) 覆盖率基线与目标
- 记录当前 server/quic 行/函数覆盖率基线；
- 目标：行/函数覆盖率相对提升（数量化在 PR 描述中给出具体值）；
- 命令：`cargo llvm-cov nextest --all-features -p silent`；
- 依赖：2)、3)、4)。

6) 文档与示例
- `docs/quic-webtransport.md`：补充“测试与稳定性”小节（无需真实握手、伪造流策略）；
- `PLAN.md`：保持与分支目标同步；
- 依赖：2)、3)；
- 验证：文档编译通过，无断链。

7) 门禁与 CI
- 本地钩子/CI 统一以 clippy `-D warnings`、deny、nextest 执行；
- nextest 配置：`.config/nextest.toml` 已知 `run-threads` 会被忽略，不影响执行；
- 依赖：全部任务；
- 验证：PR 检查通过。

## 备注
- 不进行真实 QUIC/H3 握手集成测试：保持单测可控与确定性；
- 后续可选：在 `cfg(test)` 下提供最小 `WebTransportStream` 测试适配以覆盖 handler 通路（不在本阶段强制）。
