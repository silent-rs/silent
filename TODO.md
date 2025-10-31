# TODO - Issue #1: 建立测试覆盖率和质量度量基线

> **分支**: `feature/test-coverage-baseline`
> **优先级**: P0
> **里程碑**: v2.11
> **标签**: `testing`, `infrastructure`, `good-first-issue`

## 📋 任务概述

建立项目质量基线，包括测试覆盖率统计、代码质量检查和编译性能度量。这是后续质量改进的基础。

## ✅ 任务清单

### 1. 测试覆盖率统计
- [ ] 验证 `cargo-llvm-cov` 已安装（✅ 已完成）
- [ ] 运行覆盖率测试生成报告
- [ ] 分析覆盖率数据（整体、各模块）
- [ ] 生成 HTML 可视化报告
- [ ] 生成 JSON 格式报告（供 CI 使用）
- [ ] 识别未覆盖的关键代码路径

### 2. 代码质量检查
- [ ] 运行全量 Clippy 检查（所有 features）
- [ ] 记录 Clippy 警告数量和类型
- [ ] 分类警告（性能、正确性、风格等）
- [ ] 修复或记录所有警告
- [ ] 确保 `--deny warnings` 通过

### 3. 编译性能度量
- [ ] 测量完整编译时间（clean build）
- [ ] 测量增量编译时间
- [ ] 记录各 crate 编译时间
- [ ] 记录二进制体积（release 模式）
- [ ] 识别编译瓶颈

### 4. 文档生成
- [ ] 创建 `docs/quality-metrics.md` 文档
- [ ] 记录所有质量指标基线
- [ ] 创建测试覆盖率徽章配置
- [ ] 在 README 中添加徽章
- [ ] 编写质量度量工具使用指南

### 5. 自动化与持续集成
- [ ] 创建覆盖率测试脚本 `scripts/coverage.sh`
- [ ] 创建质量检查脚本 `scripts/quality-check.sh`
- [ ] 配置 Codecov 或类似服务（可选）
- [ ] 更新 CI 工作流
- [ ] 测试 CI 流程

## 🎯 验收标准

- ✅ 测试覆盖率报告生成（HTML + JSON）
- ✅ `docs/quality-metrics.md` 完整记录所有指标
- ✅ Clippy 警告清零或有明确的处理计划
- ✅ README 中添加测试覆盖率徽章
- ✅ 所有脚本和工具可正常运行
- ✅ 文档清晰，他人可重现操作

## 📊 预期输出

### 质量指标文档应包含：
- 整体测试覆盖率（%）
- 各模块覆盖率明细
- Clippy 警告统计
- 编译时间数据
- 二进制体积数据
- 度量日期和环境信息

### 覆盖率报告应包含：
- 行覆盖率
- 函数覆盖率
- 分支覆盖率
- 未覆盖代码列表

## 🛠️ 工具和命令

### 测试覆盖率
```bash
# 运行测试并生成覆盖率报告
cargo llvm-cov --all-features --workspace --html

# 生成 JSON 格式报告
cargo llvm-cov --all-features --workspace --json --output-path coverage.json

# 查看覆盖率摘要
cargo llvm-cov --all-features --workspace
```

### 代码质量检查
```bash
# 运行 Clippy（所有 features）
cargo clippy --all-targets --all-features --tests --benches -- -D warnings

# 检查格式
cargo fmt -- --check

# 检查文档
cargo doc --all-features --no-deps
```

### 编译性能
```bash
# 完整编译时间
cargo clean && time cargo build --release

# 增量编译时间
touch silent/src/lib.rs && time cargo build --release

# 各 crate 编译时间（使用 cargo-timings）
cargo build --release --timings
```

## 📝 工作流程

### 第一步：收集数据
1. 运行覆盖率测试
2. 运行 Clippy 检查
3. 测量编译时间
4. 收集所有数据

### 第二步：分析和记录
1. 分析覆盖率数据
2. 分类 Clippy 警告
3. 识别性能瓶颈
4. 创建质量指标文档

### 第三步：改进和自动化
1. 修复关键警告
2. 创建自动化脚本
3. 更新 CI 配置
4. 更新文档

### 第四步：验证和发布
1. 验证所有工具可用
2. 检查文档完整性
3. 提交代码
4. 创建 Pull Request

## 🔍 注意事项

### 覆盖率测试
- 确保所有 features 都被测试
- 注意排除生成的代码（如果有）
- 关注核心模块的覆盖率
- 不要过度追求 100% 覆盖率

### Clippy 检查
- 优先修复 `correctness` 和 `perf` 类警告
- `style` 类警告可以按团队规范处理
- 某些警告可以通过 `#[allow(clippy::xxx)]` 忽略（需注释说明原因）
- 保持一致的代码风格

### 编译性能
- 关注最耗时的 crate
- 考虑并行编译的影响
- 记录硬件配置信息
- 多次测量取平均值

## 📅 时间估计

- 数据收集：2-3 小时
- 分析和记录：2-3 小时
- 自动化脚本：2-3 小时
- 文档编写：2-3 小时
- 测试验证：1-2 小时

**总计**: 9-14 小时（1-2 工作日）

## 🔗 相关资源

- [cargo-llvm-cov 文档](https://github.com/taiki-e/cargo-llvm-cov)
- [Clippy Lints 列表](https://rust-lang.github.io/rust-clippy/master/)
- [cargo-timings 文档](https://doc.rust-lang.org/cargo/reference/timings.html)
- [Codecov](https://codecov.io/)

## 📈 后续任务

完成后，进入 Issue #2：提升 API 文档覆盖率至 80%+

---

**创建时间**: 2025-10-31
**负责人**: @hubertshelley
**状态**: 🔄 进行中
