# TODO

## M0 验证与定稿（进行中）

完成标准：`docs/design.md` §16 每项验证有结论；§8 的决策有记录；`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo nextest run` 通过。验证结论和实测发现写在 `docs/m0-verification.md`，这里只跟踪状态。

分支：`feature/fullstack-framework`，从 `main`（d7fadde）迁出。

### 已完成

- [x] 分支与嵌套 workspace：`silent-boot/`，核心 crate `crates/silent-boot`
- [x] 迁移核心：`migration!` 宏、`FrameworkMigrator`（跟踪表 `sb_migrations`）、`MigrationRunner`（执行锁、执行记录、先框架后应用）
- [x] V1 Silent + SeaORM 2.0 最小链路
- [x] V3 自定义提取器进入元组
- [x] V4 SPA 兜底与 `<path:**>` 的优先级
- [x] V6 迁移并发（SQLite）：加锁后只执行一次；不加锁的问题已复现
- [x] V13 SQLite 上失败迁移的事务回滚
- [x] V15 两套迁移器共用一个库
- [x] V14 的基准留档：用基准命令从迁移生成的库生成了 `tests/common/entity/`

### 待办

- [ ] V2 MSRV：安装 1.94 工具链后 `cargo +1.94 check`
- [ ] V13 的 ALTER 边界：删列、改类型、重命名在 SQLite 上各自的结果
- [ ] V14 `generate entity` 的库级实现（`sea-orm-codegen` + `sea-schema`），与基准命令对比
- [ ] V16 MySQL 部分失败恢复（需要 MySQL 环境）
- [ ] V17 `cli` feature 对 release 体积与编译时间的影响
- [ ] V5 `migrate generate` 生成的文件骨架
- [ ] V7 rust-embed 前端嵌入
- [ ] V8 Tera 重载
- [ ] V9 `dev` 监听重启循环
- [ ] V10 OpenAPI 到 TS 类型
- [ ] V11 任务互斥 SQL
- [ ] V12 项目名在 crates.io 与 GitHub 上的占用
- [ ] 命令行：`migrate up` 与 `migrate status` 子命令，命中后不启动服务
- [ ] CI：给 `silent-boot/` 加 fmt、clippy、nextest
- [ ] 把实测发现 F1（提取器拒绝不进统一包络）写回 `docs/design.md` 的 §5 与 §15
- [ ] 定稿后输出 M1 的任务清单

## M1 核心底座（未开始）

见 `PLAN.md` §5 的完成标准。
