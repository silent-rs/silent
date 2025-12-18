# TODO

## 进行中

### 路由优化（`perf/route-optimizations`）

目标：提升路由匹配与分发性能，减少边界行为问题（shadowing/空段匹配）。

- ✅ 修复方法分发 `HashMap` 每次请求克隆：`silent/src/handler/handler_trait.rs`
- ✅ 动态路由按“特异性”排序，避免泛型路由抢占（如 `<id>` 抢占 `<id:i64>`）：`silent/src/route/route_service.rs`
- ✅ `<key:path>`（`*`）不匹配空段：`silent/src/route/route_tree.rs`
- ✅ 减少重复遍历：合并 `path_can_resolve` 与实际匹配流程（避免同一路径两次 DFS）：`silent/src/route/route_tree.rs`
- 🔄 优化 `Route::call` 频繁 `convert_to_route_tree` 的成本（缓存/引导使用 `RouteTree`）：`silent/src/route/mod.rs`

验收：
- `cargo fmt -- --check`
- `cargo clippy --all-targets --all-features --tests --benches -- -D warnings`

### QUIC 文档补全

- 🔄 新增 `docs/quic-cert-rotation.md`：描述 QUIC 证书切换完整流程（PLAN v2.13-M3 收尾项）

## 已完成（归档）

- ✅ Server 硬化第一阶段（配置统一与连接保护）：M1/M2/M3 基础可观测已完成
- ✅ 安全与稳定性修复：修复静态路径穿越、移除 WebSocket `unsafe Sync`、关键 `unwrap/panic` 收敛
- ✅ 关键路径 `unwrap/panic` 收敛：Session CookieJar 合并与 Worker 错误响应构造
- ✅ MSRV 与文档口径：声明 `rust-version` 并同步 README 徽章
- ✅ SocketAddr 兼容仅 IP 字符串：支持 `"127.0.0.1"` 解析为端口 0
