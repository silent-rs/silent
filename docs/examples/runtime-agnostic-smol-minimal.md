# 运行时中立最小示例（smol 入口）

示例路径：`examples/runtime_agnostic_smol_minimal/`

要点：
- 使用 `smol::block_on` 作为入口运行时。
- 用 `async-compat` 包装 `Server::serve` 以兼容内部 Tokio 传输。

运行：
- `cargo run -p example-runtime-agnostic-smol-minimal`

核心代码片段：
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
