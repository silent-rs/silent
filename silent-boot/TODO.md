# TODO

## M0 验证与定稿（收尾中）

完成标准：`docs/design.md` §16 每项验证有结论；§8 的决策有记录；`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo nextest run` 通过。验证结论和实测发现写在 `docs/m0-verification.md`，这里只跟踪状态。

分支：`feature/fullstack-framework`，从 `main`（d7fadde）迁出。

### 已完成

- [x] 分支与嵌套 workspace：`silent-boot/`，两个 crate：`crates/silent-boot`（框架）、`crates/silent-boot-cli`（全局工具）
- [x] 迁移核心：`migration!` 宏（默认在事务里执行）、`FrameworkMigrator`（跟踪表 `sb_migrations`）、`MigrationRunner`（执行锁、执行记录、先框架后应用）
- [x] 具名执行锁 `db::lock`（表 `sb_locks`）
- [x] 命令行：`migrate generate`、`migrate up`、`migrate status` 子命令；命中子命令后不启动服务
- [x] `generate entity` 的库级实现（`codegen` feature），输出与基准命令一致
- [x] `silent-boot dev` 运行循环和事件服务器
- [x] CI：`.github/workflows/silent-boot.yml`（fmt、clippy、nextest、MSRV 检查）
- [x] V1 最小链路、V2 MSRV 1.94、V3 自定义提取器进元组、V4 SPA 兜底
- [x] V5 `migrate generate` 的文件骨架与登记
- [x] V6 迁移并发（SQLite）、V11 任务互斥
- [x] V7 rust-embed 前端嵌入、V8 Tera 重载
- [x] V9 `dev` 监听重启循环
- [x] V10 OpenAPI 到 TS 类型（`openapi-typescript` 7 配 `typescript` 5，见 F15）
- [x] V12 项目名可用
- [x] V13 失败迁移的事务回滚与 ALTER 边界
- [x] V14 `generate entity` 与基准命令逐字节一致
- [x] V15 两套迁移器共用一个库
- [x] V17 `cli` 与 `codegen` 的体积，据此把代码生成拆到默认不开的 feature 里
- [x] V16 不做（D14：数据库能力交给 SeaORM）
- [x] 实测发现已写回 `docs/design.md`：F1（§5、§15）、F9（§6.6）、F10（§6.4）、F12（§12）

### 待办

- [ ] 推送分支并开草稿 PR，让 CI 工作流在 GitHub 上实际跑一遍（需要你确认后再推送）

## M1 核心底座（未开始）

完成标准见 `PLAN.md` §5。迁移核心、命令行和 `dev` 循环已经落地，M1 从启动器开始。按依赖顺序：

1. [ ] 启动器 `App`：读配置、连库、装配中间件和系统路由、挂业务路由、优雅关停
2. [ ] 约定配置 `config/app.toml` 的结构和校验，环境变量 `APP__` 前缀覆盖
3. [ ] `/healthz`、`/readyz`（有未执行的迁移就不就绪）
4. [ ] 统一响应与错误：`AppResult`、`AppError`、`ExceptionHandler`，提取器拒绝的响应也进包络（F1）
5. [ ] SeaORM 接入：`DbRegistry` 的命名连接，分页 `PageQuery`、`PageResult`
6. [ ] OpenAPI：接入 silent-openapi，`/swagger`
7. [ ] `dev` 和启动器打通：开发构建自动引入 `reload.js`，`dev --migrate`
8. [ ] 脚手架 `new`：应用骨架（分层目录、带受管区块的迁移器、Makefile）
9. [ ] 分层模板和示例模块，配 TestClient 集成测试（成功、参数错误、404）
10. [ ] `migrate lint`、`migrate baseline`、`migrate repair`
11. [ ] 带 `mysql`、`postgres` feature 时 `cargo check` 通过
12. [ ] `generate module` 的第一步：从表生成 dto、service、api
