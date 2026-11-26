# silent-boot

基于 [Silent](https://github.com/silent-rs/silent) 的全栈快速开发框架：约定优于配置的应用底座，内置 RBAC 管理后台，一条命令生成全栈模块，可多副本部署。

当前进度：**M0 已完成，M1（核心底座）本地完成**，还不能用于真实项目。M2（认证与 RBAC）还没开始。

- [PLAN.md](PLAN.md)：目标、里程碑、边界、决策记录
- [TODO.md](TODO.md)：任务清单
- [docs/design.md](docs/design.md)：技术方案（§17 是 M1 落地后和设计不一致的地方）
- [docs/m0-verification.md](docs/m0-verification.md)、[docs/m1-verification.md](docs/m1-verification.md)：每一项验证的结论和实测发现

## 现在有什么

| 能力 | 在哪 |
|---|---|
| 启动器：读配置、连库、装配中间件和路由、优雅关停；应用的入口只有一句 `silent_boot::run` | `silent_boot::app` |
| 约定配置：`config/app.toml` 加 `APP__` 前缀的环境变量，取值校验 | `silent_boot::config` |
| 统一响应与错误：`{ code, message, data, traceId }`；`AppError`；提取器的拒绝响应和 panic 都是包络 | `silent_boot::web` |
| 迁移：`migration!` 声明、`MigrationRunner` 加锁执行、对账容得下更新的数据库 | `silent_boot::db::migrate` |
| 具名执行锁（迁移和之后的定时任务共用） | `silent_boot::db::lock` |
| 子命令：`migrate generate / up / status / lint / baseline / repair`、`generate entity`、`generate module`、`healthcheck` | `silent_boot::cli` |
| 全局工具：`silent-boot new` 生成应用骨架，`silent-boot dev` 构建并运行应用、监听变更、重启、刷新页面 | `silent-boot-cli` |
| 示例应用：由 `new` 生成后提交，带着一个"文章"示例模块和接口测试 | `examples/demo` |

## 跑一遍

```bash
cd silent-boot
cargo install --path crates/silent-boot-cli     # 全局工具

silent-boot new demo && cd demo                  # 生成应用骨架
silent-boot migrate up                           # 建库、建表（在应用里执行 cargo run -- migrate up）
cargo run                                        # 启动：http://127.0.0.1:8080，文档在 /swagger
cargo test                                       # 示例模块的接口测试

silent-boot migrate generate "create order"      # 新增一条迁移：生成文件并登记
silent-boot generate entity                      # 按库结构重新生成实体（自动带上 --features codegen）
silent-boot generate module --table order        # 按表生成 dto、service、api 并登记
silent-boot dev --migrate                        # 改代码自动重启，页面自动刷新
```

和数据库打交道的子命令（`migrate …`、`generate …`）只有一个实现，在应用里（`silent_boot::cli`）：
迁移列表是应用自己的，生成器又要和应用用的 SeaORM 版本一致。全局工具在应用目录里把它们转发为
`cargo run --quiet -- <子命令>`，参数、输出、退出码原样交过去；没装全局工具时直接写 `cargo run -- …`。

生成的项目自带 `README.md` 和 `AGENTS.md`，写了怎么加迁移、生成实体和模块、各层放什么。

## 开发框架自己

这是嵌套在 Silent 仓库里的 workspace，有自己的 `Cargo.toml`，不进外层 workspace。`silent` 目前通过路径依赖引用。

```bash
cd silent-boot
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features

# 示例应用要和 `silent-boot new` 生成的一致
cd examples && cargo run -q -p silent-boot-cli -- new demo --path-framework ../crates/silent-boot --path-silent ../../silent --force
git diff --exit-code -- demo
```

改了脚手架的模板之后要同步两处：模板在 `crates/silent-boot-cli/templates/app/`，提交的示例在 `examples/demo/`
（CI 会重新生成一遍再比对）。模板里只有 `{{ name }}` 这种占位符，不留具体的取值。

## feature

| feature | 默认 | 内容 |
|---|---|---|
| `sqlite` | 开 | SQLite 驱动 |
| `mysql`、`postgres` | 关 | MySQL、PostgreSQL 驱动；框架只保证能编译（D14） |
| `cli` | 开 | 命令行解析和 `migrate`、`healthcheck` 子命令 |
| `openapi` | 开 | Swagger UI，`Envelope`、`PageResult` 可以写进 utoipa 的文档 |
| `compression` | 开 | 响应压缩 |
| `codegen` | 关 | `generate entity`、`generate module`，开发和 CI 用 `--features codegen` 打开 |
