# TODO（v2.17 极限性能优化）

> 目标版本: v2.17
> 状态: 开发中

## 上一阶段成果（v2.16 已完成 ✅）

- State 提取器（替代 Configs）
- Tower 兼容层（hook_layer）
- OpenAPI 完善（Swagger UI 嵌入、宏增强、ReDoc）
- 错误处理增强（IntoResponse trait）

## 待开发任务

### P0：热路径关键瓶颈消除

- [x] 1. Handler HashMap clone 消除 ✅
  - `handler_trait.rs` 中 `self.clone().get(&method)` → `self.get(&method)` 直接引用
  - 消除每请求的 HashMap 深拷贝开销
  - **效果：简单路由约 2x 提升**

- [x] 2. RouteTree 连接级共享 ✅
  - `route_connection.rs` 中每连接调用 `convert_to_route_tree()` → 启动时一次性构建 `Arc<RouteTree>`
  - 所有连接共享同一份冻结的路由树（HTTP 和 QUIC 均适用）

- [ ] 3. 移除 async_trait，使用原生 RPITIT
  - 因 `dyn Handler` trait object 需求，boxing 是必需的，收益有限
  - 暂缓，待后续评估

- [ ] 4. HyperService Box::pin 消除
  - hyper Service trait 要求 `type Future = Pin<Box<...>>`，无法直接消除
  - 暂缓

### P1：中间件与数据结构优化

- [x] 5. 中间件链优化 ✅
  - 无中间件时快速路径直接调用 call_children，跳过 Next 链构建
  - Next::call 中 Arc clone 改为直接引用
  - **效果：带中间件路由约 2.2x 提升**

- [x] 6. not_found_error 零分配 ✅
  - 使用 `SilentError::NotFound` 替代 `BusinessError` + String 分配
  - **效果：所有未匹配路径零字符串分配**

- [ ] 7. Request 参数容器优化
  - HashMap → SmallVec 涉及公开 API 变化，暂缓

### P2：编译与运行时调优

- [x] 8. Release profile 优化 ✅
  - `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`

- [x] 9. tracing 编译时级别控制 ✅
  - 添加 `no-tracing` feature（`tracing/max_level_off`），benchmark 时关闭 tracing

## Benchmark 结果

| 测试项 | main 基线 | 优化后 | 提升 |
|--------|-----------|--------|------|
| simple route match | 107.57 ns | 54.35 ns | ~2x |
| route with middleware | 107.50 ns | 49.29 ns | ~2.2x |
| nested route match | 133.00 ns | 91.63 ns | 31% |
| 1000 sequential requests | 184.98 µs | 132.97 µs | 28% |
| deep nested 10 levels | 206.12 ns | 166.30 ns | 19% |
| deep nested with params | 299.80 ns | 253.78 ns | 15% |
