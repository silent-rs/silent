# QUIC/HTTP3 示例需求整理

- 目标：提供一个最小可运行的 QUIC + HTTP/3 示例，演示
  - 使用 `QuicEndpointListener` 启动 QUIC 监听
  - 同端口附加 HTTPS 回退（HTTP/1.1/2）
  - 通过 `with_quic_port` 自动下发 `Alt-Svc`，便于浏览器升级到 HTTP/3

- 范围：新增 `examples/quic` 示例包，不改动现有公开 API，不影响默认构建。

- 依赖与特性：
  - 示例依赖 `silent` 的 `quic` 特性
  - 运行时需要有效证书；为便捷起见，示例复用 `examples/tls/certs/localhost+2*.pem`

- 运行方式：
  - `cargo run -p example-quic`
  - 浏览器或 `curl --http3` 访问 `https://127.0.0.1:4433/`

- 兼容性：
  - 不改变默认 feature 集；示例为独立包，符合工作区示例结构。

