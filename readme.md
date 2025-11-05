<div align="center">
<h1>Silent</h1>
<p>
<a href="https://github.com/silent-rs/silent/actions">
    <img alt="build status" src="https://github.com/silent-rs/silent/actions/workflows/build.yml/badge.svg" />
</a>
<br/>
<a href="https://crates.io/crates/silent"><img alt="crates.io" src="https://img.shields.io/crates/v/silent" /></a>
<a href="https://docs.rs/silent"><img alt="Documentation" src="https://docs.rs/silent/badge.svg" /></a>
<a href="https://deepwiki.com/silent-rs/silent"><img alt="GitWiki" src="https://img.shields.io/badge/GitWiki-Documentation-blue" /></a>
<a href="https://github.com/rust-secure-code/safety-dance/"><img alt="unsafe forbidden" src="https://img.shields.io/badge/unsafe-forbidden-success.svg" /></a>
<a href="https://www.rust-lang.org"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.75%2B-blue" /></a>
<br/>
<a href="https://crates.io/crates/silent"><img alt="Download" src="https://img.shields.io/crates/d/silent.svg" /></a>
<img alt="License" src="https://img.shields.io/crates/l/silent.svg" />
</p>
</div>

### 概要

Silent 是一个简单的基于Hyper的Web框架，它的目标是提供一个简单的、高效的、易于使用的Web框架。

### 文档

- [Crates.io](https://crates.io/crates/silent)
- [API 文档](https://docs.rs/silent)
- [GitWiki 文档](https://deepwiki.com/silent-rs/silent)
- [ZRead 文档](https://zread.ai/silent-rs/silent)
- [Cloudflare Worker 使用指南](docs/cloudflare-worker.md)

### 目标

- [x] 路由
- [x] 中间件
- [x] 静态文件
- [x] WebSocket
- [x] 模板
- [x] 日志 (使用了tracing)
- [x] 配置
- [x] 会话
- [x] 安全
- [x] GRPC
- [x] 通用网络层 (NetServer)
- [x] Cloudflare Worker

## NetServer

提供与协议无关的通用网络服务器，支持 TCP、Unix Socket 等多种监听方式，并内置连接限流和优雅关停功能。

### 基本用法

```rust
use silent::NetServer;
use std::time::Duration;

#[tokio::main]
async fn main() {
    NetServer::new()
        .bind("127.0.0.1:8080".parse().unwrap())
        .with_rate_limiter(10, Duration::from_millis(10), Duration::from_secs(2))
        .with_shutdown(Duration::from_secs(5))
        .serve(|mut stream, peer| async move {
            println!("Connection from: {}", peer);
            // 处理连接...
            Ok(())
        })
        .await;
}
```

### 功能特性

- **多监听器支持**: 同时监听多个 TCP 或 Unix Socket 地址
- **连接限流**: 基于令牌桶算法的 QPS 限制
- **优雅关停**: 支持 Ctrl-C 和 SIGTERM 信号，可配置等待时间
- **协议无关**: 通过 `ConnectionService` trait 支持任意应用层协议

### 示例

- [基本 TCP Echo 服务器](./examples/net_server_basic/)
- [自定义命令协议](./examples/net_server_custom_protocol/)

## Extractors（萃取器）

### 文档

- [萃取器完整指南](./docs/extractors-guide.md) - 详细的使用文档和最佳实践
- [API 文档](https://docs.rs/silent) - 完整的 API 参考

## security

### argon2

add make_password and verify_password function

### pbkdf2

add make_password and verify_password function

### aes

re-export aes/aes_gcm

### rsa

re-export rsa

## configs

### setting

```rust
use silent::Configs;
let mut configs = Configs::default ();
configs.insert(1i32);
```

### usage

```rust
async fn call(req: Request) -> Result<i32> {
    let num = req.configs().get::<i32>().unwrap();
    Ok(*num)
}
```

## examples for llm

* [whisper with candle](./examples/candle_whisper/readme.md)

## complex projects for llm

* [llm_server](https://github.com/silent-rs/llm_server)
