# 给 AI 助手和开发者的约定

这是一个 silent-boot 应用。改代码之前先读这一页。

## 命令

- 新增迁移：`cargo run -- migrate generate "<名称>"`，然后在生成的文件里写 `up`。
- 执行迁移：`cargo run -- migrate up`。状态：`cargo run -- migrate status`。
- 重新生成实体：`cargo run --features codegen -- generate entity`。
- 检查：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo nextest run`。
- 不要提交 `data/`、`target/`。

## 分层

请求只按一个方向走：`api → service → entity`，数据在层与层之间只以 dto 的形式出现。

| 目录 | 放什么 | 不放什么 |
|---|---|---|
| `migration/` | 数据库结构。结构只通过迁移改变 | 运行时代码 |
| `entity/` | 生成的实体，与表一一对应 | 手写的业务；不要手改，下次生成就被覆盖 |
| `src/dto/` | 入参（`CreateXxxDto`、`UpdateXxxDto`、`XxxQueryDto`）、出参（`XxxDto`）、校验规则 | `ActiveModel` |
| `src/services/` | 业务逻辑、事务、审计字段 | `Request`、`Response`；不返回 HTTP 状态码 |
| `src/api/` | 路由、提取参数、调 service、返回包络、OpenAPI 标注 | 业务逻辑；不直接操作实体 |

## 写法

- 接口的返回类型是 `Result<Envelope<T>>`（新建用 `Created<T>`），失败用 `AppError`，`?` 会自动转换。
  不要自己拼错误 JSON。
- 入参用 `ValidatedJson<T>`、`ValidatedQuery<T>` 提取，字段规则写在 `#[validate(..)]` 上；分页用 `Page` 提取器。
- 审计字段 `created_at`、`updated_at`、`created_by`、`updated_by` 在 service 里写入，SeaORM 不会自动填。
- 删除用软删除：查询时过滤 `deleted_at IS NULL`，删除时写 `deleted_at`。
- ID 用 `scru128::new_string()`，时间用 `chrono::Local::now().naive_local()`。
- 迁移里的删表、删列、改名、清空要加一行 `// silent-boot: destructive`，否则 `migrate lint` 不通过；
  破坏性变更分两次发布：先加新结构并双写，下一个版本再删旧结构。
- 每条迁移只做一件事；SQLite 上改列的类型要重建表（建新表、拷数据、删旧表、改名）。
- 配置写在 `config/app.toml`，密钥只通过环境变量给（`APP__` 前缀），不要写进文件和日志。
- `cargo run --features codegen -- generate module --table <表>` 能按表生成 dto、service、api 三个文件并自动登记；
  手写的模块自己在 `src/api/mod.rs` 里登记：`pub mod xxx;` 和 `routes()` 里各一行（写在受管区块外面）。
