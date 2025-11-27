# QUIC / HTTP/3 / WebTransport 互操作与回归矩阵

> 目标：给出一套可操作的端到端测试矩阵，覆盖 HTTP/3 / WebTransport / Datagram 在不同网络条件下的行为，便于与 curl/浏览器/quinn 等客户端进行互操作验证。

## 前置条件

- 服务端：
  - 已构建启用 `quic` feature 的可执行程序，例如 `examples/quic`。
  - 有可用证书（可复用 `examples/tls/certs/`）。
  - 推荐使用 `ServerConfig + ConnectionLimits + QuicTransportConfig` 统一配置连接/帧/Datagram 限制。
- 客户端：
  - `curl`（支持 `--http3`）。
  - 现代浏览器（支持 HTTP/3 与 WebTransport）。
  - 可选：`tc netem`（Linux）或类似工具注入高 RTT/丢包等网络条件。

所有示例假设本地监听：`127.0.0.1:4433`，路由包含：
- `GET /`：HTTP/1.1/2/3 基础路由；
- `GET /api/health`：健康检查（在 `examples/quic` 中存在）；
- WebTransport 入口：基于 `CONNECT` + `:protocol = webtransport`。

## 测试矩阵总览

| 场景 | 目标 | 客户端示例 | 预期结果 | 相关指标/日志 |
|------|------|------------|----------|---------------|
| S1. HTTP/3 基本连通性 | 验证 HTTP/3 通路与 Alt-Svc 生效 | `curl --http3 -k https://127.0.0.1:4433/` | 返回 200 + 正常响应体 | `accept.ok`、`handler.ok`、HTTP/3 响应日志 |
| S2. Alt-Svc 升级 | 验证浏览器从 H2 升级到 H3 | 浏览器访问 `https://127.0.0.1:4433/`，刷新多次 | DevTools 协议列从 `h2` 升级为 `h3`；响应头含 `Alt-Svc` | Alt-Svc 中间件 debug 日志（quic_port） |
| S3. WebTransport 会话基本收发 | 验证 WebTransport 握手与 Echo handler | 使用支持 WebTransport 的客户端 JS/工具，建立会话并发送文本帧 | 收到回显内容（含 session id）；会话正常关闭 | `webtransport.handshake_ns`、`webtransport.session_ns`，`webtransport.accept`/`error` |
| S4. Datagram send/recv | 验证 Datagram 限制与丢弃策略 | 客户端发送小型 datagram（≤ 配置上限），再发送超限 datagram | 小包正常处理，超限包被丢弃（会话不中断） | `webtransport.datagram_dropped`、`webtransport.datagram_rate_limited` |
| S5. 高 RTT 场景 | 验证 HTTP/3 与 WebTransport 在高延迟下的稳定性 | 使用 `tc netem` 注入 RTT（例如 100ms+）后重复 S1–S4 | 连接稳定，耗时增加但无大量超时 | HTTP/3 读超时计数、handler.duration_ns 分布变化 |
| S6. 丢包场景 | 验证在 1–5% 丢包下的行为与重传 | `tc netem loss 1%` + 重复 S1–S4 | 请求最终成功；偶发重试可接受 | 同上，关注错误日志与读超时计数 |
| S7. 0-RTT / 会话复用（视客户端支持而定） | 验证 0-RTT 或会话复用下行为 | 使用支持 0-RTT 的客户端（如 quinn/浏览器，按其文档配置） | 第二次连接可快速建立，业务行为与 S1 一致 | 连接建立耗时分布、日志中握手阶段耗时 |
| S8. 会话迁移（可选） | 验证客户端 IP/网络切换时连接迁移能力 | 在支持迁移的客户端中切换网络接口（如 Wi-Fi → 移动热点） | 会话不中断或仅短暂抖动 | 连接错误日志中无大量 reset/closed |
| S9. 证书切换 | 验证 QUIC 证书平滑切换流程 | 按 `docs/quic-cert-rotation.md` 运行新旧实例切换 | 切换期间连接成功率与延迟在可接受范围内 | 新旧实例均可观察到握手与会话指标 |

下面给出关键场景的简单操作方式。

## 场景 S1 / S2：HTTP/3 基础与 Alt-Svc

1. 启动示例服务（示意）：
   ```bash
   cargo run -p example-quic
   ```
2. curl 验证：
   ```bash
   curl --http3 -k https://127.0.0.1:4433/
   curl --http3 -k https://127.0.0.1:4433/api/health
   ```
3. 浏览器验证：
   - 打开 `https://127.0.0.1:4433/`（接受自签证书风险）。
   - 刷新多次，查看 DevTools 网络面板中的 `Protocol` 列，应从 `h2` 升级到 `h3`。

## 场景 S3：WebTransport 会话

> 具体客户端实现依赖浏览器或 WebTransport 库，这里只给出预期行为与观测点。

- 建立 WebTransport 会话，向服务器发送一条文本消息：
  - 服务端应回显包含 session id 的文本（例如 `session=<id> echo: <msg>`）。
  - 日志中可看到：
    - `webtransport_session` span（包含 `session_id` 和 `remote`）；
    - 会话结束时 `WebTransport 会话结束` / 异常结束时 `WebTransport 会话异常结束`。
- 观察 metrics：
  - `silent.server.webtransport.handshake_ns`：握手耗时分布。
  - `silent.server.webtransport.session_ns`：会话存活时间分布。

## 场景 S4：Datagram 限制与丢弃

1. 在服务端配置合理的 `webtransport_datagram_max_size` 与 `webtransport_datagram_rate`。
2. 客户端发送：
   - 多个小 datagram（小于等于上限）：应全部被接受（从业务视角），对应 metrics 不应出现大量丢弃。
   - 明显超限的大 datagram：应被丢弃并计数，但会话继续存在。
3. 观测指标：
   - `silent.server.webtransport.datagram_dropped`：超限或发送失败时计数。
   - `silent.server.webtransport.datagram_rate_limited`：速率限制命中计数。

## 场景 S5 / S6：高 RTT 与丢包

以 Linux `tc netem` 为例（其它平台可使用等价工具）：

```bash
# 添加高 RTT 与少量丢包
sudo tc qdisc add dev lo root netem delay 100ms 20ms loss 1%

# 运行 S1–S4 场景
curl --http3 -k https://127.0.0.1:4433/api/health

# 验证完成后移除
sudo tc qdisc del dev lo root
```

重点观测：
- HTTP/3 读超时计数是否显著增加；
- handler 耗时直方图是否符合预期（变慢但不中断）；
- 是否出现大量连接级错误或重试失败。

## 场景 S7 / S8：0-RTT 与迁移（可选）

这两类场景强依赖客户端能力，推荐做法：

- 使用支持 0-RTT / 连接迁移的 QUIC 客户端（如 quinn 客户端示例或浏览器中的实验特性）。
- 按客户端文档启用 0-RTT/迁移后，与示例服务建立连接，并执行 S1–S4 的请求/会话操作。
- 重点观测：
  - 首次连接与后续连接的建立耗时差异；
  - 在网络切换（迁移）过程中，会话是否保持，是否出现大量 RST/close 日志。

## 场景 S9：证书切换

具体步骤已在 `docs/quic-cert-rotation.md` 中给出，这里只强调验证要点：

- 在切换前后分别执行 S1–S4 场景，确认：
  - 新实例证书生效且握手成功；
  - 切换期间错误率与延迟变化在可接受范围内；
  - 老实例优雅关停，活动连接数按预期下降。

## 小结

通过上述矩阵，可以系统性地验证：
- HTTP/3 / WebTransport 在基础与恶劣网络条件下的行为；
- 连接/流/Datagram 限制是否按预期生效；
- 指标与 tracing 是否足以支撑故障分析与容量规划。

配合 `docs/quic-ops.md`、`docs/quic-webtransport.md` 与 `examples/quic`，即可完成 QUIC 模块在“测试与互操作”维度的落地验证。
