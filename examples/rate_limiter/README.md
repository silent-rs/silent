# Rate Limiter 示例

这个示例演示了如何使用 `Server` 的限流功能来控制连接速率。

## 功能特性

- **连接限流**：使用令牌桶算法限制连接速率
- **优雅关停**：在收到关停信号后等待活动连接完成

## 运行示例

```bash
cargo run -p example-rate-limiter
```

## 配置说明

### 限流配置

```rust
let rate_limiter_config = RateLimiterConfig {
    capacity: 5,                        // 容量：允许 5 个突发连接
    refill_every: Duration::from_millis(100),  // 每 100ms 补充 1 个令牌（约 10 QPS）
    max_wait: Duration::from_secs(2),   // 获取令牌最多等待 2 秒
};

.with_rate_limiter(rate_limiter_config)
```

**参数说明**：

- `capacity`: 令牌桶容量，决定了允许的最大突发连接数
- `refill_every`: 令牌补充间隔，每次补充 1 个令牌
- `max_wait`: 获取令牌的最大等待时间，超时则拒绝连接

**示例计算**：

- `refill_every = Duration::from_millis(100)` 意味着每 100ms 补充 1 个令牌
- 1 秒 = 1000ms，因此 1000ms / 100ms = 10 个令牌/秒
- 即约 **10 QPS** (Queries Per Second)

### 优雅关停配置

```rust
.with_shutdown(Duration::from_secs(10))
```

当收到关停信号（Ctrl-C 或 SIGTERM）时：
1. 停止接受新连接
2. 等待活动连接在 10 秒内完成
3. 超时后强制取消剩余连接

## 测试限流效果

### 测试突发连接

使用以下命令快速发送多个请求：

```bash
# 发送 10 个并发请求
for i in {1..10}; do curl http://127.0.0.1:8080/ & done
```

前 5 个请求应该立即被接受（突发容量），后续请求会按照约 10 QPS 的速率被处理。

### 测试等待超时

使用 `siege` 或 `ab` 进行压力测试：

```bash
# 使用 siege 测试
siege -c 20 -r 1 http://127.0.0.1:8080/

# 使用 ab 测试
ab -n 100 -c 20 http://127.0.0.1:8080/
```

当并发数超过限流配置时，超过 `max_wait` (2秒) 的请求将被拒绝。

## 相关文档

- [NetServer 限流文档](../../silent/src/server/net_server.rs)
- [Server API 文档](../../silent/src/server/mod.rs)
