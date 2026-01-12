# TODO（测试覆盖率改进）

> 分支: `feature/test-coverage-improvement`（自 `main` 切出）
> 目标版本: v2.13+
> 优先级: P1
> 状态: 🟡 进行中

## 目标
- 提升 QUIC/HTTP3 模块的测试覆盖率
- 确保核心功能路径有充分的测试覆盖
- 为低覆盖率区域补充测试用例

## 当前覆盖率基线（2025-01-09）

### QUIC 模块覆盖率
- `server/quic/core.rs`: 46.01% 行覆盖率，66.67% 函数覆盖率
- `server/quic/listener.rs`: 60.06% 行覆盖率，73.24% 函数覆盖率
- `server/quic/connection.rs`: 68.80% 行覆盖率，79.49% 函数覆盖率
- `server/quic/service.rs`: 64.56% 行覆盖率，69.14% 函数覆盖率
- `server/quic/echo.rs`: 88.81% 行覆盖率，80.00% 函数覆盖率
- `server/quic/middleware.rs`: 100.00% 行覆盖率，100.00% 函数覆盖率

### 整体覆盖率
- 总计: 61.72% 行覆盖率，60.17% 函数覆盖率
- 测试数量: 215 个测试全部通过

## 已完成任务

### ✅ 修复测试编译错误
- **问题**: `test_webtransport_handler_trait_exists` 测试中的类型推断失败
- **修复**: 添加 `?Sized` 约束到泛型类型参数
- **文件**: `silent/src/server/quic/core.rs`
- **结果**: 所有 215 个测试通过

### ✅ 补充 core.rs 测试用例
- **新增测试**: 17 个测试用例
  - 令牌补充逻辑测试（4 个）
  - 大小验证测试（4 个）
  - 速率限制测试（2 个）
  - 超时配置测试（1 个）
  - 连接可用性测试（1 个）
  - Duration 算术测试（1 个）
  - 其他边界条件测试（4 个）
- **覆盖内容**:
  - `WebTransportStream` 的令牌桶算法
  - Datagram 和帧的大小验证
  - 速率限制检查逻辑
  - 超时配置处理
  - 可选参数的处理逻辑

## 待完成任务

### 🔄 低覆盖率模块分析

#### 零覆盖率模块（需要重点关注）
1. **gRPC 模块** (0%)
   - `grpc/handler.rs`
   - `grpc/register.rs`
   - `grpc/service.rs`
   - `grpc/utils.rs`

2. **WebSocket 模块** (大部分 0%)
   - `ws/handler.rs`
   - `ws/handler_wrapper_websocket.rs`
   - `ws/message.rs`
   - `ws/route.rs`
   - `ws/upgrade.rs`
   - `ws/websocket.rs`

3. **SSE 模块** (0%)
   - `sse/event.rs`
   - `sse/keep_alive.rs`
   - `sse/reply.rs`

4. **Session 模块** (0%)
   - `session/middleware.rs`
   - `session/session_ext.rs`

5. **其他零覆盖率模块**
   - `cookie/middleware.rs`
   - `core/serde/multipart.rs`
   - `handler/handler_fn.rs`
   - `middleware/middlewares/exception_handler.rs`
   - `middleware/middlewares/request_time_logger.rs`
   - `middleware/middlewares/timeout.rs`
   - `scheduler/middleware.rs`
   - `scheduler/traits.rs`

#### 低覆盖率模块（<30%）
1. **cookie/cookie_ext.rs** (13.64%)
2. **core/form.rs** (16.88%)
3. **ws/websocket_handler.rs** (14.58%)
4. **core/path_param.rs** (23.96%)
5. **server/route_connection.rs** (25.95%)
6. **core/req_body.rs** (27.97%)
7. **core/res_body.rs** (31.52%)
8. **core/response.rs** (47.29%)
9. **route/handler_append.rs** (35.50%)

### 📋 下一步工作

#### Phase 1: QUIC 模块覆盖率提升（优先级：高）
- [ ] 为 `server/quic/core.rs` 的实际方法添加集成测试
  - `recv_data()` 方法测试
  - `try_send_datagram()` 方法测试
  - `recv_datagram()` 方法测试
  - `send_data()` 和 `finish()` 方法测试
- [ ] 为 `server/quic/listener.rs` 补充错误路径测试
- [ ] 为 `server/quic/connection.rs` 补充边界条件测试
- [ ] 为 `server/quic/service.rs` 补充端到端测试

#### Phase 2: 核心模块覆盖率提升（优先级：中）
- [ ] 为 `core/form.rs` 补充表单解析测试
- [ ] 为 `core/path_param.rs` 补充路径参数提取测试
- [ ] 为 `core/req_body.rs` 补充请求体读取测试
- [ ] 为 `core/response.rs` 补充响应构建测试

#### Phase 3: 功能模块覆盖率提升（优先级：低）
- [ ] 为 gRPC 模块添加基础测试
- [ ] 为 WebSocket 模块添加集成测试
- [ ] 为 SSE 模块添加单元测试
- [ ] 为 Session 模块添加功能测试

## 验收标准
- [ ] QUIC 模块整体行覆盖率 > 75%
- [ ] 所有测试通过 `cargo nextest run --all-features`
- [ ] 代码检查通过 `cargo clippy --all-targets --all-features --tests --benches -- -D warnings`
- [ ] 生成覆盖率报告并记录改进情况
