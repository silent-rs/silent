# 移除非Server模块对Tokio依赖的计划

## 概述
本计划旨在去除Silent框架中除server模块外其他模块对tokio的依赖，以实现更好的模块化和运行时无关性。

## 当前依赖分析

### 非Server模块中tokio的使用

#### 1. 测试相关 (大部分可替换)
- `#[tokio::test]` 属性 - 可替换为其他异步测试框架

#### 2. 实际功能使用 (需要重点处理)

| 文件 | 使用场景 | 替换方案 |
|------|----------|----------|
| `handler/handler.rs` | `async_compression::tokio::*`, `tokio::fs`, `tokio::io` | 保持静态，不依赖tokio |
| `handler/directory.rs` | `tokio::fs::read_dir` | 使用std或async-std |
| `core/request.rs` | `tokio::sync::OnceCell` | 使用`once_cell::sync::OnceCell` |
| `core/form.rs` | `tokio::fs`, `tokio::io`, `tokio::task::spawn_blocking` | 使用std同步或async-std |
| `middleware/timeout.rs` | `tokio::time::timeout` | 实现超时机制或使用async-std |
| `session/middleware.rs` | `tokio::sync::RwLock` | 使用`std::sync::RwLock` |
| `scheduler/task.rs` | `#[tokio::test]` | 替换测试框架 |
| `scheduler/mod.rs` | `tokio::sync::Mutex`, `tokio::spawn`, `tokio::time::sleep` | 使用`std` + `futures` |

## 实施步骤

### 阶段1: 依赖替换
1. 修改 `Cargo.toml`:
   - 保持 `tokio` 作为可选依赖 (`optional = true`)
   - 从非server features中移除tokio相关features
   - 添加替代依赖: `once_cell`, `async-std` 等

### 阶段2: 代码替换
1. **同步原语替换**:
   - `tokio::sync::OnceCell` → `once_cell::sync::OnceCell`
   - `tokio::sync::RwLock` → `std::sync::RwLock` 或 `async-lock`
   - `tokio::sync::Mutex` → `std::sync::Mutex` 或 `async-lock`

2. **文件系统操作替换**:
   - `tokio::fs` → `std::fs` (同步) 或 `async-std::fs`
   - `tokio::io` → `std::io` (同步) 或 `async-std::io`

3. **任务和延迟替换**:
   - `tokio::spawn` → `futures::executor::spawn` 或移除
   - `tokio::time::timeout` → 手动实现或使用`async-std::future::timeout`
   - `tokio::time::sleep` → `std::thread::sleep` (如果可同步) 或`async-std::task::sleep`

### 阶段3: 测试替换
- 将所有 `#[tokio::test]` 替换为:
  - `#[async_std::test]` (如果使用async-std)
  - 或使用 `futures-test`

### 阶段4: 验证
- 运行所有测试确保兼容性
- 验证server模块仍然正常工作
- 检查所有features是否正常

## 注意事项

1. **保持向后兼容**: 避免破坏现有API
2. **逐步替换**: 每次替换一个模块，测试通过后再进行下一个
3. **性能考虑**: 某些替换可能影响性能，需要评估
4. **特性支持**: 确保所有现有特性仍能正常工作

## 替代依赖建议

- `once_cell`: 替代 `tokio::sync::OnceCell`
- `async-std`: 替代 `tokio` 的异步功能
- `async-lock`: 异步锁替代 `tokio::sync::{RwLock,Mutex}`
- `futures`: 标准异步原语

## 风险评估

- **低风险**: 测试替换、 OnceCell 替换
- **中风险**: 文件系统操作替换、同步原语替换
- **高风险**: 调度器替换、超时机制替换

## 验证清单

- [ ] 所有非server模块不依赖tokio
- [ ] server模块仍然可以使用tokio
- [ ] 所有测试通过
- [ ] 文档测试通过
- [ ] 所有features正常工作
- [ ] 没有编译错误或警告
