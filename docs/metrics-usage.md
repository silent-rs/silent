# Metrics 使用说明（实验性）

Silent 使用 `metrics` crate 输出核心指标，用户需自行选择 exporter（如 Prometheus/OTLP）。目前默认不安装 exporter，请在应用启动时自行初始化。

## 初始化 exporter 示例（Prometheus pushgateway）

```rust
use metrics_exporter_prometheus::PrometheusBuilder;

fn init_metrics() {
    PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], 9898))
        .install()
        .expect("install prometheus recorder");
}

fn main() {
    init_metrics();
    // 启动 Silent Server...
}
```

> 也可使用 `metrics_exporter_otlp` 将数据导出至 OTLP 兼容后端（如 Tempo/Grafana Cloud 等）。

## 已输出的指标（默认标签为空，需在应用层添加）

- 计数器
  - `silent.server.accept.ok|err`
  - `silent.server.ratelimiter.closed|timeout`
  - `silent.server.handler.ok|err|timeout`
  - `silent.server.shutdown.graceful|forced`
  - `silent.server.http3.body_oversize`
  - `silent.server.webtransport.accept|error`
- 直方图（如需可在调用点启用）
  - `silent.server.handler.duration_ns`
  - `silent.server.accept.wait_ns`
  - `silent.server.shutdown.duration_ns`（label: phase）

## 标签与高基数字段

- 当前未自动附加标签，建议在应用层通过 `metrics::with_label_values!` 或为 recorder 配置全局/线程标签。
- 避免将 peer IP 等高基数字段直接作为标签，可通过日志或采样方式处理。

## 启用与关闭

- 若不需要指标，可不初始化 exporter，计数器调用开销极低。
- 若要全局关闭指标输出，可在编译时通过 Cargo feature 包装（当前未提供独立 feature，后续可按需添加）。
