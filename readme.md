<div align="center">
<h1>Silent</h1>
<p>
<a href="https://github.com/silent-rs/silent/actions"><img alt="build status" src="https://github.com/silent-rs/silent/actions/workflows/build.yml/badge.svg" /></a>
<a href="https://crates.io/crates/silent"><img alt="crates.io" src="https://img.shields.io/crates/v/silent" /></a>
<a href="https://docs.rs/silent"><img alt="Documentation" src="https://docs.rs/silent/badge.svg" /></a>
<a href="https://deepwiki.com/silent-rs/silent"><img alt="GitWiki" src="https://img.shields.io/badge/GitWiki-Documentation-blue" /></a>
<a href="https://github.com/rust-secure-code/safety-dance/"><img alt="unsafe forbidden" src="https://img.shields.io/badge/unsafe-forbidden-success.svg" /></a>
<a href="https://www.rust-lang.org"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.85%2B-blue" /></a>
<br/>
<a href="https://zread.ai/silent-rs/silent" target="_blank"><img src="https://img.shields.io/badge/Ask_Zread-_.svg?style=flat&color=00b0aa&labelColor=000000&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB3aWR0aD0iMTYiIGhlaWdodD0iMTYiIHZpZXdCb3g9IjAgMCAxNiAxNiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHBhdGggZD0iTTQuOTYxNTYgMS42MDAxSDIuMjQxNTZDMS44ODgxIDEuNjAwMSAxLjYwMTU2IDEuODg2NjQgMS42MDE1NiAyLjI0MDFWNC45NjAxQzEuNjAxNTYgNS4zMTM1NiAxLjg4ODEgNS42MDAxIDIuMjQxNTYgNS42MDAxSDQuOTYxNTZDNS4zMTUwMiA1LjYwMDEgNS42MDE1NiA1LjMxMzU2IDUuNjAxNTYgNC45NjAxVjIuMjQwMUM1LjYwMTU2IDEuODg2NjQgNS4zMTUwMiAxLjYwMDEgNC45NjE1NiAxLjYwMDFaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00Ljk2MTU2IDEwLjM5OTlIMi4yNDE1NkMxLjg4ODEgMTAuMzk5OSAxLjYwMTU2IDEwLjY4NjQgMS42MDE1NiAxMS4wMzk5VjEzLjc1OTlDMS42MDE1NiAxNC4xMTM0IDEuODg4MSAxNC4zOTk5IDIuMjQxNTYgMTQuMzk5OUg0Ljk2MTU2QzUuMzE1MDIgMTQuMzk5OSA1LjYwMTU2IDE0LjExMzQgNS42MDE1NiAxMy43NTk5VjExLjAzOTlDNS42MDE1NiAxMC42ODY0IDUuMzE1MDIgMTAuMzk5OSA0Ljk2MTU2IDEwLjM5OTlaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik0xMy43NTg0IDEuNjAwMUgxMS4wMzg0QzEwLjY4NSAxLjYwMDEgMTAuMzk4NCAxLjg4NjY0IDEwLjM5ODQgMi4yNDAxVjQuOTYwMUMxMC4zOTg0IDUuMzEzNTYgMTAuNjg1IDUuNjAwMSAxMS4wMzg0IDUuNjAwMUgxMy43NTg0QzE0LjExMTkgNS42MDAxIDE0LjM5ODQgNS4zMTM1NiAxNC4zOTg0IDQuOTYwMVYyLjI0MDFDMTQuMzk4NCAxLjg4NjY0IDE0LjExMTkgMS42MDAxIDEzLjc1ODQgMS42MDAxWiIgZmlsbD0iI2ZmZiIvPgo8cGF0aCBkPSJNNCAxMkwxMiA0TDQgMTJaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00IDEyTDEyIDQiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSIxLjUiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIvPgo8L3N2Zz4K&logoColor=ffffff" alt="zread"/></a>
<a href="https://crates.io/crates/silent"><img alt="Download" src="https://img.shields.io/crates/d/silent.svg" /></a>
<img alt="License" src="https://img.shields.io/crates/l/silent.svg" />
</p>
</div>

## 概要

Silent 是基于 Hyper 的纯 Web 框架，专注于 Web 协议、路由、请求响应、服务端传输、实时协议基础、测试工具和稳定扩展入口。

- [Crates.io](https://crates.io/crates/silent)
- [API 文档](https://docs.rs/silent)
- [项目规划](PLAN.md)
- [需求整理](docs/requirements.md)
- [Cloudflare Worker 指南](docs/cloudflare-worker.md)

## 核心能力

- HTTP/1.1、HTTP/2、HTTP/3、TLS、QUIC 和通用 `NetServer`；
- 高性能路由、中间件、请求、响应和通用提取器；
- WebSocket、SSE、流式响应和静态资源；
- Cookie、State、请求扩展和 TestClient；
- Tower、Cloudflare Worker 与独立组件所需的公共扩展入口。

## 项目边界

认证与权限、生产会话存储、监控导出器、可靠任务调度、数据库接入、管理界面、MQTT 实现和项目脚手架不进入 Silent 核心，由独立仓库自行维护。

规划中的独立生态包括：`silent-auth`、`silent-session`、`silent-observability`、`silent-scheduler`、`silent-admin`、`silent-seaorm` 和 `silent-cli`。MQTT 已由 [silent-mqtt](https://github.com/silent-rs/silent-mqtt) 独立维护。尚未创建的仓库不提供占位链接。

`silent-openapi` 位于当前工作区，但作为独立 crate 发布并维护自己的兼容关系。

## 2.x 兼容能力

现有 `session`、`scheduler`、`security`、`template` 和 `grpc` feature 在整个 2.x 保持可用：

- `session` 和 `scheduler` 是兼容保留入口，只做必要修复，不继续扩展生产存储或可靠调度能力；
- `security` 是通用密码与加密工具，不是认证、用户或权限系统；
- `security`、`template` 和 `grpc` 的长期归属由 3.0 RFC 决定，本路线不预先承诺迁出或移除；
- `admin` 只是 `server + sse + template + session` 的兼容聚合 feature，不包含管理后台或管理界面；
- `Configs`、`RequestTimeLogger` 等已弃用入口在 2.x 继续保留，最早于 3.0 移除。

破坏性调整只会在替代能力、迁移指南、兼容验证和公开 RFC 完成后进入 3.0。

## 快速开始

```rust
use silent::prelude::*;

async fn hello(_req: Request) -> Result<&'static str> {
    Ok("Hello, Silent!")
}

#[tokio::main]
async fn main() {
    let app = Route::new_root().append(Route::new("hello").get(hello));

    Server::new()
        .bind("127.0.0.1:8080".parse().unwrap())
        .serve(app)
        .await;
}
```

## State

应用级共享数据使用 `Route::with_state` 注入，通过 `Request::get_state` 或 `State<T>` 提取器读取：

```rust
use silent::prelude::*;

#[derive(Clone)]
struct AppConfig {
    name: &'static str,
}

async fn handler(req: Request) -> Result<String> {
    let config = req.get_state::<AppConfig>()?;
    Ok(format!("hello {}", config.name))
}

let app = Route::new_root()
    .with_state(AppConfig { name: "Silent" })
    .get(handler);
```

`Configs`、`get_config` 和 `configs` 仅为 2.x 兼容保留，新代码应使用 State API。

## NetServer

`NetServer` 提供与具体应用层协议无关的 TCP/Unix Socket 监听、连接限流和优雅关停。具体协议实现由独立项目负责。

```rust,no_run
use silent::{BoxedConnection, NetServer, RateLimiterConfig, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() {
    let handler = |mut stream: BoxedConnection, _peer: SocketAddr| async move {
        let mut buf = [0_u8; 1024];
        let n = stream.read(&mut buf).await?;
        stream.write_all(&buf[..n]).await?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    };

    let rate_limit = RateLimiterConfig {
        capacity: 100,
        refill_every: Duration::from_millis(10),
        max_wait: Duration::from_secs(1),
    };

    NetServer::new()
        .bind("127.0.0.1:8080".parse().unwrap())
        .unwrap()
        .with_rate_limiter(rate_limit)
        .with_shutdown(Duration::from_secs(30))
        .serve(handler)
        .await;
}
```

示例：

- [基本 TCP Echo 服务器](examples/net_server_basic/)
- [自定义命令协议](examples/net_server_custom_protocol/)
- [萃取器指南](docs/extractors-guide.md)
- [OpenAPI 组件](silent-openapi/README.md)

## 开发检查

```bash
cargo fmt -- --check
cargo check --all
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo nextest run --all-features
cargo deny check
```

项目采用 Apache-2.0 许可证。
