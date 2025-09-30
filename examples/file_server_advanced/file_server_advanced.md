# example-file_server_advanced 使用说明

更新时间：2025-09-29 13:07:55

## 功能概述

该示例展示如何基于 Silent 的静态资源能力一次性开启压缩传输与目录浏览，并附加健康检查接口，便于在真实项目中快速验证静态服务能力。

## 目录结构

```text
examples/file_server_advanced/
├── Cargo.toml
└── src
    └── main.rs
```

运行时会自动创建 `static-advanced/` 目录，并写入 `index.html` 以及 `docs/readme.txt` 内容用于目录浏览演示。

## 关键代码

```rust
let options = StaticOptions::default()
    .with_directory_listing()
    .with_compression();

let route = Route::new("")
    .append(Route::new("health").get(|_req: Request| async move { Ok::<_, SilentError>("ok") }))
    .with_static_options(static_root, options);
```

- `with_directory_listing` 启用目录索引模式，默认返回带 `./`、`../` 导航的 HTML。
- `with_compression` 根据客户端 `Accept-Encoding` 自动协商 `gzip`/`brotli`。
- `Route::new("health")` 提供简单健康检查接口，便于在部署环境做探活。

## 运行方式

```bash
cargo run -p example-file_server_advanced --features static
```

- 首次运行会生成 `static-advanced/` 目录及示例文件。
- 访问 `http://127.0.0.1:3000/` 可查看目录索引或首页。
- 访问 `http://127.0.0.1:3000/health` 返回 `ok` 表示服务正常。
