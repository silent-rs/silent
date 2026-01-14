# TODO（测试覆盖率改进）

> 分支: `feature/test-coverage-improvement`（自 `main` 切出）
> 目标版本: v2.13+
> 优先级: P1
> 状态: 🟡 进行中

## 目标
- 提升 QUIC/HTTP3 模块的测试覆盖率
- 确保核心功能路径有充分的测试覆盖
- 为低覆盖率区域补充测试用例

## 当前覆盖率基线（2025-01-14）

### QUIC 模块覆盖率
- `server/quic/core.rs`: 64.33% 行覆盖率，77.08% 函数覆盖率 ⬆️ (+18.32%)
- `server/quic/listener.rs`: 77.33% 行覆盖率，82.24% 函数覆盖率 ⬆️ (+17.27%)
- `server/quic/connection.rs`: 83.28% 行覆盖率，86.96% 函数覆盖率 ⬆️ (+83.28%)
- `server/quic/service.rs`: 73.07% 行覆盖率，75.93% 函数覆盖率 ⬆️ (+8.51%)
- `server/quic/echo.rs`: 88.81% 行覆盖率，80.00% 函数覆盖率
- `server/quic/middleware.rs`: 100.00% 行覆盖率，100.00% 函数覆盖率

### 整体覆盖率
- 总计: 70.89% 行覆盖率，69.78% 函数覆盖率 ⬆️
- 测试数量: 463 个测试全部通过 ⬆️ (+184 个测试)

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

### ✅ 大幅提升 listener.rs 测试覆盖率（2025-01-14）
- **新增测试**: 30 个测试用例
- **覆盖内容**:
  - QuicTransportConfig 配置测试（10 个）
  - 地址验证和绑定测试（4 个）
  - 错误处理模式测试（5 个）
  - 类型安全和边界条件（11 个）
- **覆盖率提升**:
  - 行覆盖率：60.06% → 77.33%（+17.27%）✅
  - 函数覆盖率：73.24% → 82.24%（+9.00%）
- **提交**: c8c676f

### ✅ 大幅提升 connection.rs 测试覆盖率（2025-01-14）
- **新增测试**: 27 个测试用例
- **覆盖内容**:
  - AsyncRead/AsyncWrite trait 测试（10 个）
  - Pin 和所有权测试（8 个）
  - Context 和类型安全（9 个）
- **覆盖率提升**:
  - 行覆盖率：0% → 83.28%（+83.28%）⭐
  - 函数覆盖率：0% → 86.96%（+86.96%）⭐
- **提交**: c8c676f

### ✅ 提升 service.rs 测试覆盖率（2025-01-14）
- **新增测试**: 13 个测试用例
- **覆盖内容**:
  - 边界条件和特殊情况测试（5 个）
  - 限制和验证测试（3 个）
  - 测试工具验证（2 个）
  - 地址变化测试（1 个）
  - 性能测试（2 个）
- **覆盖率提升**:
  - 行覆盖率：64.56% → 73.07%（+8.51%）
  - 函数覆盖率：69.14% → 75.93%（+6.79%）
  - 区域覆盖率：72.93% → 80.94%（+8.01%）
- **提交**: 33708b5

### ✅ 大幅提升 core/form.rs 测试覆盖率（2025-01-14）
- **新增测试**: 33 个测试用例
- **覆盖内容**:
  - FormData 构造函数测试（2 个）
  - FormData::read() 边界条件测试（3 个）
  - FilePart getter 方法测试（7 个）
  - FilePart::save() 方法测试（2 个）
  - FilePart::do_not_delete_on_drop() 测试（1 个）
  - FilePart 内存布局测试（2 个）
  - MultiMap 集成测试（2 个）
  - 边界条件和错误处理测试（3 个）
  - FormData 和 FilePart 类型测试（2 个）
  - HeaderMap 集成测试（1 个）
  - 文件路径处理测试（1 个）
  - 临时目录管理测试（2 个）
  - 重复字段测试（1 个）
  - 文件名变更和特殊情况测试（3 个）
  - 多字段组合测试（1 个）
- **覆盖率提升**:
  - 行覆盖率：16.88% → 91.62%（+74.74%）⭐
  - 函数覆盖率：~0% → 91.53%（+91.53%）⭐
  - 区域覆盖率：93.33%⭐
- **测试数量**: 279 → 356（+77 个测试，其中 33 个来自 form.rs）
- **提交**: a43ee56

### ✅ 大幅提升 core/path_param.rs 测试覆盖率（2025-01-14）
- **新增测试**: 66 个测试用例
- **覆盖内容**:
  - PathParam From trait 实现测试（6 个）
  - PathParam borrowed_str/borrowed_path 测试（2 个）
  - TryFrom 转换测试（i32/i64/u64/u32/String/Uuid）（14 个）
  - PathString 方法测试（borrowed/as_str/as_cow）（5 个）
  - PathSlice 方法测试（as_str/source/range）（3 个）
  - Debug trait 测试（3 个）
  - Clone trait 测试（3 个）
  - PartialEq trait 测试（4 个）
  - 边界条件测试（5 个）
  - Arc 共享测试（1 个）
  - Unicode 和特殊字符测试（3 个）
  - Path vs Str 区别测试（2 个）
  - 大数值测试（2 个）
  - 错误类型验证测试（1 个）
  - Range 边界测试（2 个）
  - 多实例比较测试（1 个）
- **覆盖率提升**:
  - 行覆盖率：23.96% → 98.07%（+74.11%）⭐
  - 函数覆盖率：30.00% → 100.00%（+70.00%）⭐
  - 区域覆盖率：25.32% → 98.60%（+73.28%）⭐
- **测试数量**: 356 → 422（+66 个测试）
- **提交**: b28ea07

### ✅ 大幅提升 core/req_body.rs 测试覆盖率（2025-01-14）
- **新增测试**: 41 个测试用例
- **覆盖内容**:
  - ReqBody::Empty 测试（2 个）
  - ReqBody::Once 测试（3 个）
  - From<()> trait 测试（1 个）
  - with_limit 方法测试（3 个）
  - from_stream 方法测试（1 个）
  - Debug trait 测试（5 个）
  - SizeHint 测试（2 个）
  - is_end_stream 测试（3 个）
  - poll_frame 测试（3 个）
  - poll_next 测试（2 个）
  - LimitedIncoming 测试（2 个）
  - Bytes 相关测试（5 个）
  - 边界条件测试（2 个）
  - 类型验证测试（2 个）
  - Trait 边界测试（2 个）
  - 行为测试（3 个）
  - 格式验证测试（1 个）
  - 等价性测试（1 个）
- **覆盖率提升**:
  - 行覆盖率：30.77% → 84.54%（+53.77%）⭐
  - 函数覆盖率：38.46% → 89.55%（+51.09%）⭐
  - 区域覆盖率：33.67% → 85.61%（+51.94%）⭐
- **测试数量**: 422 → 463（+41 个测试）
- **提交**: 00cc27c

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

#### Phase 1: QUIC 模块覆盖率提升（优先级：高）✅ 已完成 4/4
- [x] 为 `server/quic/listener.rs` 补充错误路径测试（✅ 已完成）
- [x] 为 `server/quic/connection.rs` 补充边界条件测试（✅ 已完成）
- [x] 为 `server/quic/service.rs` 补充端到端测试（✅ 已完成）
- [x] 为 `server/quic/core.rs` 的实际方法添加集成测试（✅ 已完成）

#### Phase 1 验收结果：✅ 已达成目标
- QUIC 模块 4/6 文件达到 75% 以上覆盖率
- 整体行覆盖率：70.89%
- 函数覆盖率：69.78%
- 测试数量：279 个（+64 个测试）

#### Phase 2: 核心模块覆盖率提升（优先级：中）✅ 3/4 完成
- [x] 为 `core/form.rs` 补充表单解析测试（✅ 已完成，16.88% → 91.62%）
- [x] 为 `core/path_param.rs` 补充路径参数提取测试（✅ 已完成，23.96% → 98.07%）
- [x] 为 `core/req_body.rs` 补充请求体读取测试（✅ 已完成，30.77% → 84.54%）
- [ ] 为 `core/response.rs` 补充响应构建测试（当前 47.29%）

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
