# TODO（测试覆盖率改进）

> 分支: `feature/test-coverage-improvement`（自 `main` 切出）
> 目标版本: v2.13+
> 优先级: P1
> 状态: 🟡 进行中

## 目标
- 提升整体模块测试覆盖率到 85% 以上
- 确保核心功能路径有充分的测试覆盖
- 为零覆盖和低覆盖率区域补充测试用例

## 当前整体覆盖率（2025-01-26 更新）

### 总体指标
- **区域覆盖率**: 84.51%
- **函数覆盖率**: 81.55%
- **行覆盖率**: 82.46%
- **测试数量**: 1019 个测试全部通过 ✅

### 各模块覆盖率现状

#### ✅ 已完成模块（>85% 行覆盖率）

**核心模块**：
- `core/form.rs`: 93.33% ✅
- `core/path_param.rs`: 98.60% ✅
- `core/req_body.rs`: 85.85% ✅
- `core/request.rs`: 85.45% ✅
- `core/res_body.rs`: 87.37% ✅
- `core/response.rs`: 98.50% ✅
- `core/serde/mod.rs`: 88.43% ✅
- `core/serde/multipart.rs`: 90.60% ✅

**中间件模块**：
- `cookie/cookie_ext.rs`: 100.00% ✅
- `cookie/middleware.rs`: 97.96% ✅
- `middleware/middlewares/cors.rs`: 96.31% ✅
- `middleware/middlewares/exception_handler.rs`: 90.53% ✅
- `middleware/middlewares/request_time_logger.rs`: 100.00% ✅
- `middleware/middlewares/timeout.rs`: 97.87% ✅

**WebSocket 模块**：
- `ws/handler.rs`: 100.00% ✅
- `ws/message.rs`: 98.63% ✅
- `ws/upgrade.rs`: 85.82% ✅

**gRPC 模块**：
- `grpc/utils.rs`: 99.63% ✅

**调度器模块**：
- `scheduler/middleware.rs`: 100.00% ✅
- `scheduler/traits.rs`: 100.00% ✅
- `scheduler/mod.rs`: 88.16% ✅
- `scheduler/task.rs`: 89.52% ✅
- `scheduler/process_time.rs`: 75.56% ✅

**其他模块**：
- `configs/mod.rs`: 81.67% ✅
- `extractor/from_request.rs`: 91.11% ✅
- `extractor/mod.rs`: 89.29% ✅
- `handler/handler_fn.rs`: 95.17% ✅
- `handler/handler_trait.rs`: 100.00% ✅
- `handler/handler_wrapper.rs`: 100.00% ✅
- `handler/static/options.rs`: 100.00% ✅
- `handler/static/handler.rs`: 88.93% ✅
- `middleware/middleware_trait.rs`: 100.00% ✅
- `route/handler_match.rs`: 100.00% ✅
- `route/route_service.rs`: 88.89% ✅
- `server/connection.rs`: 96.77% ✅
- `server/connection_service.rs`: 100.00% ✅
- `server/listener.rs`: 69.37% ⚠️
- `server/net_server.rs`: 81.71% ✅
- `server/protocol/hyper_http/mod.rs`: 93.51% ✅
- `server/route_connection.rs`: 68.46% ⚠️
- `server/stream.rs`: 86.60% ✅

**QUIC 模块**：
- `server/quic/connection.rs`: 84.43% ✅
- `server/quic/echo.rs`: 88.81% ✅
- `server/quic/listener.rs`: 77.54% ✅
- `server/quic/middleware.rs`: 100.00% ✅

---

## 待完成任务

### 🔴 零覆盖率模块（优先级：P0）

#### 1. Session 模块（0%）
- `session/middleware.rs` - 0.00% (24 行)
- `session/session_ext.rs` - 0.00% (24 行)

#### 2. SSE 模块（0%）
- `sse/event.rs` - 0.00% (69 行)
- `sse/keep_alive.rs` - 0.00% (54 行)
- `sse/reply.rs` - 0.00% (24 行)

#### 3. WebSocket 模块（部分 0%）
- `ws/handler_wrapper_websocket.rs` - 0.00% (35 行)
- `ws/route.rs` - 0.00% (34 行)
- `ws/websocket.rs` - 0.00% (81 行)
- `ws/websocket_handler.rs` - 14.58% (41/48 行未覆盖)

### 🟡 低覆盖率模块（<70%，优先级：P1）

#### 核心模块
- `core/remote_addr.rs` - 59.32% (24/59 行未覆盖)
- `core/socket_addr.rs` - 56.00% (22/50 行未覆盖)
- `error/mod.rs` - 64.86% (26/74 行未覆盖)

#### gRPC 模块
- `grpc/handler.rs` - 58.08% (83/198 行未覆盖)
- `grpc/register.rs` - 71.05% (33/114 行未覆盖)
- `grpc/service.rs` - 51.32% (37/76 行未覆盖)

#### 路由模块
- `route/handler_append.rs` - 58.03% (175/417 行未覆盖，86/129 函数未覆盖)
- `route/mod.rs` - 73.84% (90/344 行未覆盖)

#### 服务器模块
- `server/mod.rs` - 44.74% (63/114 行未覆盖)
- `server/config.rs` - 52.94% (8/17 行未覆盖)
- `server/metrics.rs` - 66.67% (29/87 行未覆盖)
- `server/tls.rs` - 63.67% (101/278 行未覆盖)

#### 静态文件处理
- `handler/static/compression.rs` - 62.67% (28/75 行未覆盖)
- `handler/static/directory.rs` - 79.71% (14/69 行未覆盖)

#### QUIC 模块（需要小幅改进）
- `server/quic/core.rs` - 64.33% (117/328 行未覆盖)
- `server/quic/service.rs` - 73.07% (241/895 行未覆盖)

#### 其他
- `templates/middleware.rs` - 71.95% (23/82 行未覆盖)
- `server/protocol/hyper_http/hyper_service.rs` - 70.77% (19/65 行未覆盖)
- `route/route_tree.rs` - 78.42% (104/482 行未覆盖)
- `core/next.rs` - 91.30% (2/23 行未覆盖) - 接近目标

---

## 工作计划

### Phase 1: 零覆盖率模块（优先级最高）

#### 1.1 WebSocket 模块剩余文件
- [ ] `ws/handler_wrapper_websocket.rs` - 0% → 75%+
- [ ] `ws/route.rs` - 0% → 75%+
- [ ] `ws/websocket.rs` - 0% → 75%+
- [ ] `ws/websocket_handler.rs` - 14.58% → 75%+

#### 1.2 SSE 模块
- [ ] `sse/event.rs` - 0% → 75%+
- [ ] `sse/keep_alive.rs` - 0% → 75%+
- [ ] `sse/reply.rs` - 0% → 75%+

#### 1.3 Session 模块
- [ ] `session/middleware.rs` - 0% → 75%+
- [ ] `session/session_ext.rs` - 0% → 75%+

### Phase 2: 低覆盖率核心模块（优先级：P1）

#### 2.1 核心模块
- [ ] `core/remote_addr.rs` - 59.32% → 75%+
- [ ] `core/socket_addr.rs` - 56.00% → 75%+
- [ ] `error/mod.rs` - 64.86% → 75%+

#### 2.2 gRPC 模块（补充测试）
- [ ] `grpc/handler.rs` - 58.08% → 75%+
- [ ] `grpc/register.rs` - 71.05% → 75%+
- [ ] `grpc/service.rs` - 51.32% → 75%+

#### 2.3 路由模块
- [ ] `route/handler_append.rs` - 58.03% → 75%+
- [ ] `route/mod.rs` - 73.84% → 75%+

### Phase 3: 其他模块优化（优先级：P2）

#### 3.1 服务器模块
- [ ] `server/mod.rs` - 44.74% → 70%+
- [ ] `server/config.rs` - 52.94% → 70%+
- [ ] `server/metrics.rs` - 66.67% → 70%+
- [ ] `server/tls.rs` - 63.67% → 70%+

#### 3.2 静态文件处理
- [ ] `handler/static/compression.rs` - 62.67% → 75%+
- [ ] `handler/static/directory.rs` - 79.71% → 85%+

#### 3.3 QUIC 模块（小幅改进）
- [ ] `server/quic/core.rs` - 64.33% → 70%+
- [ ] `server/quic/service.rs` - 73.07% → 75%+

---

## 验收标准

### 主要目标
- [ ] 整体行覆盖率 > 85%
- [ ] 所有零覆盖率模块达到 75% 以上
- [ ] 所有测试通过 `cargo nextest run --all-features`
- [ ] 代码检查通过 `cargo clippy --all-targets --all-features --tests --benches -- -D warnings`

### 次要目标
- [ ] gRPC 模块整体行覆盖率 > 75%
- [ ] WebSocket 模块整体行覆盖率 > 85%
- [ ] 路由模块整体行覆盖率 > 75%

---

## 下一步行动

建议按照以下顺序进行开发：

1. **立即开始**: WebSocket 模块剩余文件（4 个文件，预计 +150 个测试）
2. **第二阶段**: SSE 模块（3 个文件，预计 +60 个测试）
3. **第三阶段**: Session 模块（2 个文件，预计 +40 个测试）
4. **第四阶段**: 低覆盖率核心模块和 gRPC 模块优化
5. **最后阶段**: 其他模块优化和整体调优

预计总测试数将从 **1019** 提升到 **1300+**，整体行覆盖率从 **82.46%** 提升到 **85%+**。
