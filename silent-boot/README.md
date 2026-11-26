# silent-boot

基于 [Silent](https://github.com/silent-rs/silent) 的全栈快速开发框架：约定优于配置的应用底座，内置 RBAC 管理后台，一条命令生成全栈模块，可多副本部署。

当前处于 **M0（验证与定稿）**，还不能用于真实项目。

- [PLAN.md](PLAN.md)：目标、里程碑、边界、决策记录
- [TODO.md](TODO.md)：任务清单
- [docs/design.md](docs/design.md)：技术方案
- [docs/m0-verification.md](docs/m0-verification.md)：每一项验证的结论和实测发现

## 现在有什么

- `db::migrate`：迁移核心。`migration!` 声明一条迁移，并默认让它在事务里执行；`FrameworkMigrator` 管框架自己的迁移，跟踪表是 `sb_migrations`；`MigrationRunner` 加锁后先执行框架迁移、再执行应用迁移，并把每次执行记到 `sb_migration_log`。
- `db::DbRegistry`：数据库连接集合，目前只有默认连接。
- `cli`：应用二进制的子命令入口。第一个参数不是框架的子命令时返回 `None`，应用照常启动服务；`migrate up`、`migrate status` 已经可用。
- `cli::generate`（`codegen` feature）：`generate entity`，按库结构生成实体文件，输出与 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 一致。

## feature

| feature | 默认 | 内容 |
|---|---|---|
| `sqlite` | 开 | SQLite 驱动 |
| `cli` | 开 | 命令行解析和 `migrate` 子命令 |
| `codegen` | 关 | `generate entity`，开发和 CI 用 `--features codegen` 打开 |

## 开发

这是嵌套在 Silent 仓库里的 workspace，有自己的 `Cargo.toml`，不进外层 workspace。`silent` 目前通过路径依赖引用。

```bash
cd silent-boot
cargo fmt --all -- --check
cargo clippy -p silent-boot --all-targets --all-features -- -D warnings
cargo nextest run -p silent-boot --all-features
```
