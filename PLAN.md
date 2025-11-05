# 项目规划（运行时无关化）

## 愿景与目标
- 将非 server 模块与 Tokio 运行时解耦，提升可移植性与可维护性。
- 保持 server 模块依赖 Tokio 以确保性能与生态兼容。

## 里程碑
- v2.11：抽象运行时接口（spawn/sleep），模块开始去 Tok io 化。
- v2.12：完成 sse、static、multipart、scheduler、grpc 等模块的去 Tok io 化。

## 优先级
1. sse 定时器去 Tokio 化（Timer）
2. static/目录流式输出去 Tokio 化（async-fs + futures-io）
3. multipart 写文件去 Tokio 化（async-fs）
4. scheduler 锁与定时去 Tokio 化（async-lock + Timer）
5. grpc 互斥锁与任务派发去 Tokio 化（async-lock + runtime::spawn）

## 技术选型
- 定时器：`async-io::Timer`
- 文件 IO：`async-fs`
- 压缩：`async-compression`（`futures-io` 后端）
- 锁：`async-lock::Mutex`
- 任务派发：`crate::runtime::spawn`（底层使用 Tokio）

## 关键时间节点
- 2025-11-05：完成首批模块重构并通过 cargo check
