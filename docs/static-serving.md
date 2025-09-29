# 静态资源服务使用说明

更新时间：2025-09-29 13:07:55

## 功能概述

Silent 在启用 `static` feature 后，提供基于路由树的静态文件分发能力。框架会按请求路径解析出文件相对路径，从指定根目录读取文件并以流的形式返回，同时自动推断 `Content-Type`，未命中文件则返回 404。

## 启用条件

- Cargo 需为 `silent` 启用 `static` feature，例如：

```toml
[dependencies]
silent = { version = "*", features = ["server", "static"] }
```

或在命令行加上 `--features static`（`full` feature 已包含 `static`）。
- 运行时依赖 Tokio（`static` feature 会自动启用 `tokio/fs`）。
- 静态根目录必须存在，构造 handler 时会直接 panic 不存在的路径。

## 路由挂载方式

- 根目录静态托管：

```rust
use silent::prelude::*;

fn main() {
    logger::fmt().init();
    let route = Route::new("").with_static("static");
    Server::new().run(route);
}
```

- 子路径挂载：

```rust
let route = Route::new("")
    .append(Route::new("api").get(handler))
    .with_static_in_url("assets", "public");
```

`Route::with_static(path)` 等价于追加 `Route::new("<path:**>")` 并绑定 GET 处理器；`with_static_in_url(url, path)` 则将静态目录挂载到 `/{url}` 前缀。若需要自定义行为，可使用 `with_static_options`/`with_static_in_url_options` 并配合 `StaticOptions`。

```rust
use silent::prelude::*;

let options = StaticOptions::default()
    .with_compression()
    .with_directory_listing();

let route = Route::new("")
    .with_static_options("static", options);
```

## 路径与文件解析

- 请求路径会被解码（支持 URL 编码），然后与根目录拼接为真实文件路径。
- 默认情况下，当路径为空或以 `/` 结尾时自动补全 `index.html`；启用目录浏览后则返回目录索引。
- 仅响应 GET 请求；其他方法需自行追加处理器。
- 静态目录之外的路径访问（如 `..`）不会被额外过滤，请务必提供受信任的根目录或自行做白名单校验。

## 可选能力

- **按需压缩**：
  - 调用 `StaticOptions::with_compression` 开启，根据 `Accept-Encoding` 自动在 `gzip`/`br` 中协商。
  - 仅对常见文本类 MIME 类型压缩，并自动补充 `Content-Encoding` 与 `Vary: Accept-Encoding`。

- **目录浏览**：
  - 调用 `StaticOptions::with_directory_listing` 启用后，目录请求直接返回带 `./` 导航的索引页面，根目录默认不再展示 `../` 链接。
  - 返回内容为简易 HTML，链接遵循相对路径，可配合前端样式自行美化。
  - 启用后将不再查找 `index.html` 默认页。

## 响应行为

- 命中文件：
  - 根据扩展名推断 `Content-Type`，缺省为 `application/octet-stream`。
  - 通过 `tokio_util::io::ReaderStream` 以流式响应，适合大文件。
- 未命中文件或解码失败：返回 `SilentError::BusinessError`，HTTP 状态码为 404，消息为 `Not Found`。

## 示例与验证

- 仓库自带 `examples/file_server` 展示最小可运行样例：

```bash
cargo run -p example-file-server --features static
```

- `examples/file_server_advanced` 覆盖目录浏览与压缩组合能力，并提供健康检查：

```bash
cargo run -p example-file_server_advanced --features static
```

- 推荐在完成路由集成后执行 `cargo check --features static`，确保编译通过并启用对应依赖。

## 常见问题

- **目录不存在**：`static_handler` 构造时直接 panic，请在启动前创建目录。
- **Vary 头缺失**：压缩能力自动注入 `Vary: Accept-Encoding`，如服务链路中存在代理请确认未被覆盖。
- **目录浏览安全**：开启目录浏览后会暴露目录结构和文件名称，请谨慎选择可访问的静态根目录，必要时通过中间件过滤敏感文件。
- **哈希路由前端**：`with_static` 支持将 SPA 构建产物挂载到根或子路由，404 时不会回退到 HTML，需要自行提供兜底文件。
