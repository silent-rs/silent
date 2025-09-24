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
- [x] Cloudflare Worker

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
