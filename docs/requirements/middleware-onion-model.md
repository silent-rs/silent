# 中间件执行模型重构（洋葱模型）

更新时间：2025-08-27

## 背景与目标

- 取消在路由匹配后一次性“收集中间件”的做法。
- 匹配到每一层路由节点时，立刻按注册顺序执行该节点的中间件，并将“向下匹配/调用”的逻辑作为被包裹的下一层，从而形成真正的洋葱模型。
- 即使后续更深层路由匹配失败，也要确保已匹配到的中间件可以被执行（包裹到最终结果，例如 404）。

## 变更内容

- 移除 `MiddleWareHandler` trait 上的 `match_req` 方法。
- `RouteTree` 执行逻辑改为：
  - 先匹配当前节点；
  - 以“继续向下匹配/回退到当前处理器”的处理器作为端点，按注册顺序包裹当前节点的中间件并执行；
  - 端点内部遵循原 DFS 语义：优先尝试子节点，失败时根据 `**`（FullPath）规则回退到当前节点处理器，均会被上一层中间件包裹。

## 兼容性与迁移指南

- 破坏性变更：`MiddleWareHandler::match_req` 被移除。
  - 迁移方式：将原先 `match_req` 的条件判断内联到 `handle` 中：
    - 命中条件时直接处理并返回；
    - 否则调用 `next.call(req).await` 继续后续处理。

示例（原基于 `match_req` 的过滤）：

```rust
#[async_trait]
impl MiddleWareHandler for YourMw {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        if should_handle(&req) {
            // 处理并返回
            return Ok(build_response());
        }
        // 不命中则透传
        next.call(req).await
    }
}
```

## 行为确认

- 中间件执行顺序：保持“注册顺序即执行顺序”。
- 层级包裹顺序：父节点中间件在外层，子节点在内层，最终包裹到终点处理器。
- 匹配失败时：已匹配过的父层中间件仍会执行，并包裹错误结果（如 404）。

## 相关检查

- `cargo check`/`cargo clippy` 通过。
- 受影响子包：`silent-openapi` 已将 `match_req` 逻辑迁移至 `handle` 中。
