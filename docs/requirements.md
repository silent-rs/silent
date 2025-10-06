# 需求整理

## 背景
当前 `silent/src/core/adapt.rs` 混合了承载协议抽象与 hyper HTTP 适配的代码，难以扩展其他协议实现。

## 功能范围
- 为核心模块增加协议层实现，抽象出对协议适配所需的接口。
- 将 hyper HTTP 适配相关的 adapt 实现迁移到独立目录中，理清职责边界。
- 保持现有对外 API 行为不变。
- 扩展 `Protocol` trait 的通用性，使其既可服务于 HTTP，也能在 MQTT 等自定义协议场景中复用。
- 在 `silent-mqtt` 中实现基于 `Protocol` trait 的 MQTT 适配器，完成报文解析与响应编码。

## 验收标准
- 重构后的代码能够通过 `cargo check`。
- 新的目录结构清晰分离协议抽象与 hyper 实现，核心模块引用路径正确。
