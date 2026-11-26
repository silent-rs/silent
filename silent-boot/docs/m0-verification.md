# M0 验证记录

> 对应 `docs/design.md` §16。状态：进行中，更新于 2026-11-25。
>
> 标"实测"的内容都是在本机跑出来的，测试文件都写在 `crates/silent-boot/tests/` 里，可以直接重跑。

## 验证项

| # | 事项 | 结论 | 依据 |
|---|---|---|---|
| V1 | Silent + SeaORM 2.0 最小链路 | 通过 | `web.rs::v1_minimal_chain_crud_through_test_client`：`with_state` 注入连接，`State`、`Json`、`Query` 提取器 handler，TestClient 走通列表、新增、过滤，以及 400、422、404 三条失败路径 |
| V2 | MSRV 1.94 | 未做 | 本机没有 1.94 工具链，要先 `rustup toolchain install 1.94` |
| V3 | 自定义提取器进入元组 | 通过 | `web.rs::v3_custom_extractor_joins_extractor_tuple`：`CurrentUser` 实现 `FromRequest`，参与三元组 handler，拿不到时返回 401 |
| V4 | SPA 兜底与 `<path:**>` 优先级 | 通过（自带兜底） | `web.rs::v4_spa_fallback_leaves_api_and_static_assets_alone`：`/api/*` 不受影响，`/articles/123` 回退到 `index.html`，`/assets` 前缀下的静态资源由 Silent 的静态处理器返回，缺失的仍是 404 |
| V5 | sea-orm-cli 的 `migrate generate` | 未做 | |
| V6 | 迁移并发 | 通过（SQLite） | `migration.rs::v6_lock_lets_only_one_runner_apply_migrations`；不加锁的对照组 `v6_repro_two_runners_without_lock`（手动运行） |
| V7 | rust-embed 前端嵌入 | 未做 | |
| V8 | Tera 重载 | 未做 | |
| V9 | `dev` 监听重启循环 | 未做 | |
| V10 | OpenAPI 到 TS 类型 | 未做 | |
| V11 | 任务互斥 SQL | 未做 | |
| V12 | 项目名占用 | 未做 | `cargo search silent-boot` 当天没有返回结果，需要再查一次 |
| V13 | SQLite 上的迁移事务与 ALTER 边界 | 事务部分通过，ALTER 边界未做 | `migration.rs::v13_failed_migration_rolls_back_on_sqlite_only_inside_transaction` |
| V14 | 应用内 `generate entity` | 基准已留档，库级实现未做 | 用基准命令从迁移生成的库得到了 `tests/common/entity/`（`article.rs`、`lib.rs`、`prelude.rs`） |
| V15 | 两套迁移器共用一个库 | 通过 | `migration.rs::v15_framework_and_app_migrators_share_one_database` |
| V16 | MySQL 部分失败恢复 | 未做 | 需要 MySQL 环境 |
| V17 | `cli` feature 的代价 | 未做 | |

## 实测发现

需要写回设计或记住的结论。

### F1 提取器拒绝的响应不经过 ExceptionHandler

- `Json`、`Query` 这些提取器失败时，响应体是纯文本的消息：Content-Type 不对是 400 `content-type is error`，请求体不是合法 JSON 是 422，内容是 serde 的原始错误（如 `expected ident at line 1 column 2`）。
- 自定义提取器的 `Rejection` 也一样：拒绝响应被直接当成正常响应返回，`ExceptionHandler` 看到的是 `Ok`。
- 只有两类错误会变成统一包络：handler 返回的 `Err`，以及未匹配路由的 404（实测为 `{"code":404,"message":"not found"}`）。
- 影响：`docs/design.md` §5 的"统一包络"要另外覆盖提取器拒绝。可行的做法有两种，框架自己的提取器把 `Rejection` 定成能渲染包络的类型；或者加一层中间件，把 4xx、5xx 且不是 JSON 的响应体改写成包络。这一条同时是对 Silent 的反馈。

### F2 不加锁时并发迁移的真实表现

两个连接同时对同一条待执行的迁移调用 SeaORM 的 `up`，两个都会执行迁移体，后写跟踪表的那个报 `UNIQUE constraint failed: seaql_migrations.version`。

- 迁移在事务里执行（`migration!` 的默认值）：失败的那个连接写入的数据随事务回滚。数据没有重复，但命令以失败告终。
- 迁移不包事务（SeaORM 在 SQLite 上的默认值）：失败的那个连接写入的数据留了下来。同一条迁移等于执行了两次，除了那个报错之外没有任何提示。

这说明执行锁是必需的，`migration!` 默认开事务也有价值：即使有人绕过执行器，数据也不会重复。

### F3 基准命令的输出

`sea-orm-cli generate entity --with-serde both -v -o <目录> -l`（2.0.4）对一个带审计字段的 `article` 表生成三个文件：`article.rs`、`lib.rs`、`prelude.rs`。`article.rs` 依次是 `Model`、`Relation` 和最后一行 `impl ActiveModelBehavior for ActiveModel {}`，没有显式的 `ActiveModel` 结构体。

迁移里的列类型和生成的 Rust 类型对应如下：`string()`、`text()` 是 `String`，`tiny_integer()` 是 `i8`，`date_time()` 是 `DateTime`。

`-v` 的输出里，连接、发现过程、每个表的列、写入的文件名是信息行；另外有大量 sqlx 的调试日志。

### F4 静态资源

- Silent 静态处理器给 `.js` 的 Content-Type 是 `text/javascript; charset=utf-8`。
- `StaticOptions` 没有 SPA 回退，自己加一个 `<path:**>` 兜底 handler 是可行的，见 V4。

### F5 测试夹具的坑

SQLite 的数据库文件所在目录被删除以后，已经打开的连接还能读，但写入会报 `attempt to write a readonly database`。夹具返回的 `TempDir` 必须在测试期间一直持有。

### F6 `macro_rules!` 里引用 `async_trait`

`sea_orm_migration::prelude::*` 带进来的 `async_trait` 是模块名，不是属性宏。通过 glob re-export 去引用属性宏，会得到一连串误导性的错误（E0726 隐式生命周期、E0195 与 trait 签名不匹配），真正的原因是属性没有生效。正确做法是显式 `pub use async_trait::async_trait;`，`db::migrate` 里已经这样处理。

### F7 SeaORM 2.0.4 与 1.x 不同的 API

- `ConnectionTrait::execute`、`query_one`、`query_all` 接受语句的引用；原生 SQL 用 `execute_raw`、`query_one_raw`、`query_all_raw`。
- `SelectStatement` 用 `.from()`，不是 `.from_table()`。
- 每个实体都要显式有一份 `impl ActiveModelBehavior for ActiveModel {}`，没有兜底实现。
- 实现 `FromRequest` 时返回类型要写 `std::result::Result`，Silent prelude 里的 `Result` 是只有一个参数的别名。

## 复现命令

```bash
cd silent-boot
cargo test -p silent-boot                              # 进 CI 的验证
cargo test -p silent-boot --test migration v6_repro -- --ignored --nocapture   # F2 的对照组

# 用迁移生成样例库，再用基准命令生成实体（V14 的基准）
SB_SAMPLE_DB=/tmp/sb-m0-sample/article.db cargo test -p silent-boot --test tools write_sample_db -- --ignored
sea-orm-cli generate entity --with-serde both -v -o crates/silent-boot/tests/common/entity -l \
    --ignore-tables seaql_migrations,sb_migrations,sb_migration_log,sb_migration_lock \
    -u "sqlite:///tmp/sb-m0-sample/article.db?mode=rw"
```
