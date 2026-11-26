# TODO

## M0 验证与定稿（进行中）

完成标准：`docs/design.md` §16 每项验证有结论；§8 的决策有记录；`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo nextest run` 通过。验证结论和实测发现写在 `docs/m0-verification.md`，这里只跟踪状态。

分支：`feature/fullstack-framework`，从 `main`（d7fadde）迁出。

### 已完成

- [x] 分支与嵌套 workspace：`silent-boot/`，核心 crate `crates/silent-boot`
- [x] 迁移核心：`migration!` 宏（默认在事务里执行）、`FrameworkMigrator`（跟踪表 `sb_migrations`）、`MigrationRunner`（执行锁、执行记录、先框架后应用）
- [x] `db::DbRegistry`：默认连接
- [x] 命令行：`migrate generate`、`migrate up`、`migrate status` 子命令；命中子命令后不启动服务
- [x] `generate entity` 的库级实现（`codegen` feature），输出与基准命令一致
- [x] CI：`.github/workflows/silent-boot.yml`（fmt、clippy、nextest、MSRV 检查）
- [x] V1 Silent + SeaORM 2.0 最小链路
- [x] V2 MSRV 1.94
- [x] V3 自定义提取器进入元组
- [x] V4 SPA 兜底与 `<path:**>` 的优先级
- [x] V5 `migrate generate` 的文件骨架与登记
- [x] V6 迁移并发（SQLite）：加锁后只执行一次；不加锁的问题已复现
- [x] V7 rust-embed 前端嵌入（debug 读磁盘）
- [x] V8 Tera 重载
- [x] V12 项目名可用
- [x] V13 SQLite 上失败迁移的事务回滚，以及 ALTER 的边界
- [x] V14 `generate entity` 与基准命令逐字节一致
- [x] V15 两套迁移器共用一个库
- [x] V17 `cli` 与 `codegen` 的体积，据此把代码生成拆到默认不开的 feature 里
- [x] 实测发现已写回 `docs/design.md`：F1（§5、§15）、F9（§6.6）、F10（§6.4）

### 待办

- [ ] V9 `dev` 监听重启循环
- [ ] V10 OpenAPI 到 TS 类型
- [ ] V11 任务互斥 SQL
- [ ] V16 MySQL 部分失败恢复（需要 MySQL 环境）
- [ ] M0 收尾后输出 M1 的任务清单

## M1 核心底座（未开始）

见 `PLAN.md` §5 的完成标准。已经落地的迁移核心和命令行是 M1 的一部分，M1 从启动器、脚手架 `new` 和分层模板开始。
