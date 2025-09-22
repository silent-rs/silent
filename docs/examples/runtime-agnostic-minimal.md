# 运行时中立最小示例（非 Tokio 入口）

本示例展示在不使用 Tokio 作为入口运行时的情况下，如何启动 Silent。

推荐使用 smol 作为非 Tokio 入口示例（仓库内已提供）：
- 示例路径：`examples/runtime_agnostic_smol_minimal/`
- 运行：`cargo run -p example-runtime-agnostic-smol-minimal`

要点：
- 使用 `smol::block_on` 作为入口运行时。
- 使用 `async-compat` 包装 `Server::serve`，以兼容内部基于 Tokio 的 HTTP 传输后端。

核心代码片段（smol 入口）：
```rust
fn main() {
    silent::logger::fmt().init();
    smol::block_on(async {
        let route = silent::Route::new("").get(|_req: silent::Request| async { Ok("ok") });
        async_compat::Compat::new(async move {
            silent::Server::new().serve(route).await;
        })
        .await;
    });
}
```
