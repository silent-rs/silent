# TODO（v2.16 开发计划）

> 目标版本: v2.16
> 状态: 开发中

## 上一阶段成果（v2.15 已完成 ✅）

- TestClient 集成测试工具（TestClient/TestRequest/TestResponse）
- 路由性能优化（freeze 模式消除请求级深拷贝，180x 提升）
- Cloudflare Worker 生态增强（with_configs、Context 注入、KV/D1/R2 示例）
- Logger 中间件（结构化 tracing 字段，替代 RequestTimeLogger）

## 待开发任务

### P0：State 提取器（替代 Configs）✅

- [x] 1. 新增 `State` 容器类型（替代 `Configs` 结构体）
  - `configs/mod.rs` 中 `Configs` 重命名为 `State`
  - `Configs` 作为 deprecated 类型别名保留
- [x] 2. 新增 `State<T>` 提取器（`extractor/types.rs`）
  - 实现 FromRequest trait，从 Request 内部状态容器中提取
  - `Configs<T>` 提取器标记 deprecated
- [x] 3. Request 内部 `configs` 字段重命名为 `state`
  - 新增 `get_state<T>()` / `state()` / `state_mut()` 方法
  - 旧方法 `get_config` / `configs()` / `configs_mut()` 标记 deprecated
- [x] 4. Response 同步更新
  - 新增 `get_state<T>()` / `state()` / `state_mut()` 方法
  - 旧方法标记 deprecated
- [x] 5. Route / RouteTree 支持 State 注入
  - `with_state<T>(val: T)` 泛型链式方法，支持任意类型直接注入
  - 支持链式调用 `.with_state(a).with_state(b)`
  - 内部 `configs` 字段重命名为 `state`
- [x] 6. Cloudflare Worker 适配
  - WorkRoute `with_state<T>(val: T)` 泛型方法
  - 保留 `with_configs()` 但标记 deprecated
- [x] 7. 示例更新
  - configs / extractors / cloudflare-worker / openapi 示例全部迁移到新 API
- [x] 8. 淘汰时间线
  - v2.16: Configs 标记 deprecated，State 作为新 API
  - v2.18: 移除 Configs（与 RequestTimeLogger 淘汰节奏一致）

### P1：Tower 兼容层

- [ ] 待细化

### P2：OpenAPI 完善

- [ ] 待细化

### P3：错误处理增强

- [ ] 待细化
