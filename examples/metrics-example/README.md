# metrics-example

演示如何为 Silent 初始化 `metrics` 体系与 Prometheus exporter，并启动 HTTP 端暴露指标。

## 运行

```bash
cargo run -p example-metrics
```

- 业务服务：`http://127.0.0.1:8080/`（返回 `ok`）
- 指标端点：`http://127.0.0.1:9898/metrics`

> 如需对接 OTLP，可将 `metrics_exporter_prometheus` 替换为 `metrics_exporter_otlp` 并配置对应后端。
