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
- `cli`：应用二进制的子命令入口。第一个参数不是框架的子命令时返回 `None`，应用照常启动服务；`migrate generate`、`migrate up`、`migrate status` 已经可用。
- `cli::generate`（`codegen` feature）：`generate entity`，按库结构生成实体文件，输出与 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 一致。
- `db::lock`：具名执行锁，一张表里的租约行。迁移执行器和之后的定时任务共用。
- `silent-boot-cli`（另一个 crate，全局工具）：`silent-boot dev`。

## dev 运行循环

在应用的目录里运行：

```bash
cargo run -p silent-boot-cli -- dev --bin <可执行目标> --port 9230
```

它会构建并启动应用，监听 `src/`、`entity/src/`、`migration/src/`、`templates/` 和 `Cargo.toml`；有变更就重新构建、重启应用，并通知页面刷新。启动时它会打印一个脚本地址，开发时在页面里引入即可：

```html
<script src="http://127.0.0.1:9230/__dev__/reload.js"></script>
```

后端重启后页面自动刷新；构建失败时错误会盖在页面上，旧进程保持不动。

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
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
```
