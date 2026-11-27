# M1 验证记录

> 对应 `PLAN.md` §5 里 M1 的完成标准。状态：**本地全部通过**，更新于 2026-11-26。
> CI 工作流（含示例应用的作业）还没在 GitHub 上跑过：分支没有推送。
>
> 标"实测"的内容都是在本机跑出来的。测试在 `crates/silent-boot/tests/`、`crates/silent-boot-cli/tests/` 和
> `examples/demo/tests/` 里，可以直接重跑。

## 完成标准逐条对照

| 完成标准 | 结论 | 依据 |
|---|---|---|
| `new` 出的项目 5 分钟内可运行 | 通过 | `silent-boot new demo` 生成 26 个文件，`cargo run -- migrate up`、`cargo run` 之后 `/healthz`、`/api/articles`、`/swagger` 都能用。首次编译要拉依赖，依赖缓存热了之后到启动不到 1 分钟 |
| 内置示例模块的 CRUD 有 TestClient 集成测试（成功、参数错误、404） | 通过 | `examples/demo/tests/api.rs` 的 4 个测试：CRUD 全流程、422 的字段错误与 400、404 的包络、系统路由与文档 |
| `/swagger` 可访问 | 通过 | `tests/app.rs::v24_cors_compression_and_openapi_follow_config`：`/swagger/` 返回带 `SwaggerUIBundle` 的页面，`openapi.json` 里有接口和 `Envelope<…>` 的类型；线上环境默认关闭 |
| SQLite 上从零建库通过 | 通过 | `tests/app.rs::v18`：`auto_apply = "always"` 启动时建出 `data/app.db`；示例应用上 `migrate up` 实测 |
| 带 `mysql`、`postgres` feature 时能编译 | 通过 | `cargo clippy -p silent-boot --all-targets --no-default-features --features mysql,postgres,cli -- -D warnings`；CI 里有同一条 |
| 两进程并发执行迁移只跑一次 | 通过 | `tests/cli_dispatch.rs::v35_two_processes_migrate_at_once_and_only_one_applies`：两个真实的 `sb-sample` 进程同时 `migrate up`，一个执行、另一个没事可做，锁表里不留残余。同进程的两连接版本是 `tests/migration.rs` 的 V6 |
| "旧版本建库后升级"测试通过 | 通过 | `tests/migration.rs::upgrading_a_database_created_by_an_old_release_keeps_the_data`：旧版本建库、写入数据，新版本升级后数据保留，旧副本照常运行、照常就绪；分叉的版本被拒绝（见 F16） |
| `generate entity` 对同样的表输出与基准命令一致 | 通过（M0） | `tests/generate.rs::v14`；CI 里对示例应用跑一遍 `generate entity --dry-run`，确认实体模板和生成器的输出一致 |
| `fmt`、`clippy -D warnings`、`nextest` 全部通过 | 通过 | 嵌套 workspace：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo nextest run --workspace --all-features`（94 个测试）；另跑了 `sqlite` 和 `mysql,postgres,cli` 两组 feature 的 clippy，以及默认 feature 的测试 |
| 生成项目 `cargo check` 零警告 | 通过 | `examples/demo`：`cargo clippy --all-targets -- -D warnings` 干净，`cargo fmt --check` 干净 |
| MSRV 1.94 | 通过 | `rustc 1.94.1` 下 `cargo check --workspace --all-features --all-targets` 通过，示例应用的 `cargo check --all-targets` 也通过。工具链装在临时目录里，见"复现命令" |
| `generate module` 的第一步 | 通过 | `tests/generate_module.rs::v36_generate_module_rebuilds_the_sample_module_from_its_table`：把示例应用拷一份、删掉手写的模块，用生成器按 `article` 表补回来，这份拷贝 clippy 干净、示例自带的 4 个接口测试照常通过 |
| `migrate lint`、`baseline`、`repair` | 通过 | `tests/cli_dispatch.rs` 的 V28–V31 和 `cli::lint` 的单元测试 |
| `dev` 和启动器打通 | 通过 | `tests/dev_startup.rs` 的 4 个测试；在示例应用上实测，见 F13 的补充 |
| 全局工具转发数据库相关的子命令 | 通过 | `crates/silent-boot-cli/tests/forwarding.rs` 的 4 个测试：从子目录里执行 `silent-boot migrate up`、`migrate status --json`、`migrate lint`、`generate entity --dry-run`（自动带 `codegen`）、`healthcheck` 的退出码原样返回；不在应用里时说明原因 |

## 实测发现

M0 的发现在 `m0-verification.md`（F1–F15）。以下是 M1 新增的，编号接着排。

### F16 SeaORM 的迁移器容不下"数据库比这个版本新"

SeaORM 的 `up` 和 `status` 在对账时，发现跟踪表里有当前版本不认识的迁移就直接报错
（`Migration file of version '…' is missing`）。写"旧版本建库后升级"的测试时暴露出来：新版本一执行迁移，
还在跑的旧副本就执行不了 `migrate up`，`/readyz` 也跟着报未就绪，滚动发布走不通。

处理：框架自己算"这个版本里有、跟踪表里还没有"的迁移（`MigrationDiff`），不认识的迁移只在 `migrate status`
里列成 `newer`，不算问题；执行循环由框架写，只借用 SeaORM 的迁移 trait、建表和事务约定。
一个和线上分叉的版本（既缺线上已执行的迁移，又有自己没执行的迁移）被拒绝执行，提示换最新的版本。

SeaORM 没有公开 `exec_up_with`，这一点记在 `design.md` §15 的 U8。

### F17 执行锁靠 Drop 释放不可靠

`migrate up` 执行完，进程随即退出。原来锁在 `Drop` 里排一个后台任务去释放，进程退出时这个任务不一定来得及跑：
两个真实进程同时迁移时，后到的那个要等到租约过期（10 分钟）才拿得到锁，表现为等 30 秒后超时失败。

处理：`MigrationRunner::up` 在返回前显式 `await` 释放。同样的两个进程，测试从 30 秒超时失败变成 0.7 秒通过。

### F18 `#[serde(flatten)]` 的分页字段读不到查询串里的数字

查询串里的值都是字符串，`serde(flatten)` 会先把它们收集成 map 再交给内层结构体，内层的 `Option<u64>` 拿到的是
字符串，反序列化失败（`invalid type: string "1", expected u64`）。

处理：分页参数不放进 DTO，单独做成 `Page` 提取器，自己解析 `page`、`page_size` 并校验。

### F19 派生宏展开后按 crate 名引用，应用必须直接依赖这些 crate

`utoipa` 的 `ToSchema`、`OpenApi`，`validator` 的 `Validate`，`rust-embed` 的 `RustEmbed`，
SeaORM 的 `DeriveMigrationName`，展开后都会写出 `::crate_name::…` 的路径。只靠 silent-boot 再导出不够，
应用（和它的迁移 crate）要把它们写成直接依赖。脚手架已经这样生成，版本号有测试守着。

### F20 空路径的路由节点会抢在按段匹配之前接走请求

Silent 的路由树里，路径为空的节点是"匹配一切"的动态子节点。`Route::new("").append(系统路由)` 这样套一层，
会让它在 `/healthz` 这种按段匹配之前先被选中，结果是 404。启动器现在把系统路由、业务路由、文档和静态资源
一个个直接挂在根路径（或者 `base_path`）下，不再套空路径。

### F21 Swagger UI 按完整路径认请求

`SwaggerUiHandler` 拿请求的完整路径和自己初始化时的路径比较。应用配了 `base_path = "/admin"` 之后，
要把 `/admin/swagger` 传给它，挂载点的节点名只用最后一段。

### F22 Silent 的 `Server` 没有可以编程触发的关停

`Server::serve` 只认 Ctrl-C 和 SIGTERM，测试用的关停通知是私有的。启动器的 `App::serve(listener, shutdown)`
 在外面用 `select!` 等一个关停信号，信号来了就丢掉 serve 的 future。这种方式不等在途请求；
要优雅关停就发信号。记在 `design.md` §15 的 U9。

### F23 rustfmt 从标准输入读源码时按 2015 版解析

`rustfmt < 文件.rs` 不带 `--edition` 时按 2015 版解析，`async fn` 直接报错。`generate entity` 没碰到是因为
实体文件里没有 `async`；`generate module` 一出来就撞上了。现在固定传 `--edition 2024`。

### F24 `generate module` 对示例表的输出

对 `article` 表生成的模块和手写的示例模块行为一致：示例应用自带的 4 个接口测试在生成的模块上照常通过，
`clippy -D warnings` 干净。差别只有两处：404 的说明是表名（`article不存在`，示例里是 `文章不存在`），
以及必填字符串的校验只有 `min = 1`（示例的 `title` 列在迁移里是 `string()`，没有长度上限）。

### F25 `dev` 在示例应用上的实测

`silent-boot dev --migrate`：第一次构建之后执行 `migrate up`（空的库建出全部的表），启动应用，
页面里出现 `<script src="http://127.0.0.1:9230/__dev__/reload.js" async></script>`；
改一行源码之后 2.8 秒页面收到 `event: reload`（其中约 0.5 秒是沙箱里轮询的间隔）；Ctrl-C 之后应用一起停掉；给应用发 SIGTERM 时它走 Silent 的优雅关停（`shutdown signal received`），dev 循环随之结束。

### F26 根 `.gitignore` 把脚手架模板里的 `AGENTS.md` 一起忽略了

仓库根的 `.gitignore` 有一行 `AGENTS.md`（开发者本地的代理规则，不入库），它对子目录同样生效：
`crates/silent-boot-cli/templates/app/AGENTS.md` 和 `examples/demo/AGENTS.md` 都没有被跟踪（模板目录 26 个文件只进去了 25 个）。
本机有这两个文件，所以一切正常；CI 检出后 `include_str!` 找不到模板，`silent-boot-cli` 根本编译不过。
处理：加 `silent-boot/.gitignore`（一行 `!AGENTS.md`）把这个子树里的 `AGENTS.md` 重新包含进来，模板和示例里的那份都已入库。

### F27 应用子命令要在应用目录里执行，`--manifest-path` 不改变应用根目录

`app_root()` 从当前目录向上找 `silent-boot.toml`，找不到就用当前目录。CI 的示例作业原来在 `silent-boot/` 下用
`--manifest-path examples/demo/Cargo.toml` 跑 `migrate up` 和 `generate entity --dry-run`：cargo 知道要构建哪个包，
但应用根目录落到了 `silent-boot/`——库建在 `silent-boot/data/app.db`，实体核对的是不存在的 `silent-boot/entity/src`，
三个文件都报 `would change`，这一步必然失败。处理：这两步改在 `examples/demo` 里执行，和 README 写给用户的命令一致。
`generate entity --dry-run` 对"文件不存在"和"内容不同"都报 `would change`，作为核对信号两者都算不一致。

### F28 CI 的 `check` 作业偶发卡住：`generate_is_forwarded_with_the_codegen_feature_and_exit_codes_are_kept`

PR #236 的第二次运行里，`cargo nextest run --workspace --all-features` 这一步其余 100 个测试通过，这一个测试一直没有结束
（nextest 每 60 秒报一次 `SLOW`，到 900 秒时手动取消）。第一次运行和取消后的重跑都在 130 到 180 秒内通过；
本机把这两个转发测试放在一起并行跑也是 46 秒通过，没有复现。取消后的日志里只有 `SLOW` 行：转发时子进程的输出被测试
捕获，看不到卡在哪一步（等构建目录的锁、编译，还是应用子命令本身）。已知的相关事实：这个测试和
`migrate_is_forwarded_from_a_nested_directory_with_its_arguments` 并行执行，都在 `examples/demo` 里 `cargo run`，
共用同一个构建目录的锁，一个在编译时另一个只能等；三次运行里这两个测试和 V36 的耗时在 128 到 181 秒之间，
随 runner 快慢浮动很大。根因还没有定位。可选的处理（都没做，等你定）：把这两个测试放进 nextest 的同一个测试组串行执行；
给 CI 的测试步骤加 `timeout-minutes`，免得挂到作业的 6 小时上限；给转发测试加超时，超时的时候把子进程树打进失败信息里。

## 复现命令

```bash
cd silent-boot

# 进 CI 的验证
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy -p silent-boot --all-targets --no-default-features --features sqlite -- -D warnings
cargo clippy -p silent-boot --all-targets --no-default-features --features mysql,postgres,cli -- -D warnings
cargo nextest run --workspace --all-features

# 示例应用：提交的内容和 `silent-boot new` 生成的一致，然后 lint、测试、核对实体
cd examples
cargo run -q -p silent-boot-cli -- new demo --path-framework ../crates/silent-boot --path-silent ../../silent --force
git diff --exit-code -- demo
cargo clippy --manifest-path demo/Cargo.toml --all-targets -- -D warnings
cargo nextest run --manifest-path demo/Cargo.toml
# 应用子命令从当前目录向上找 silent-boot.toml 来定位应用根目录，所以要进到应用里执行
cd demo
cargo run -q -- migrate up
cargo run -q --features codegen -- generate entity --dry-run
cd ../..

# V36：generate module（会拷一份示例应用、删掉手写模块、再用生成器补回来）
cargo nextest run -p silent-boot --all-features -E 'test(v36)'

# MSRV：本机 ~/.rustup 不可写，把工具链装到临时目录；下载走 USTC 镜像
export RUSTUP_HOME=/tmp/sb-rustup RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
rustup toolchain install 1.94 --profile minimal
cargo +1.94 check --workspace --all-features --all-targets
cargo +1.94 check --manifest-path examples/demo/Cargo.toml --all-targets

# F25：dev 循环。在应用的目录里运行全局工具（装了 silent-boot-cli 之后就是 `silent-boot dev --migrate`）。
# 另开一个终端改 src/ 下的文件，或者访问 http://127.0.0.1:9230/__dev__/events
cd examples/demo && cargo run -q --manifest-path ../../Cargo.toml -p silent-boot-cli -- dev --migrate
```
