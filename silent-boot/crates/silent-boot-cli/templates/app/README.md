# {{ title }}

用 [silent-boot](https://github.com/silent-rs/silent) 生成的后台服务：Silent + SeaORM，迁移是数据库结构的唯一事实来源，
接口统一返回 `{ code, message, data, traceId }`，自带一个"文章"示例模块。

## 跑起来

```bash
cargo run -- migrate up      # 建库、建表（SQLite 文件在 data/app.db）
cargo run                    # 启动，http://127.0.0.1:8080
```

打开 http://127.0.0.1:8080/swagger 调试接口，http://127.0.0.1:8080 是一个调接口的占位页面。

改代码想自动重启：

```bash
cargo install silent-boot-cli    # 全局工具，只需要装一次
silent-boot dev --migrate
```

装了全局工具之后，下面的命令都可以把 `cargo run --features codegen --` 换成 `silent-boot`：
`silent-boot migrate up`、`silent-boot migrate generate "create order"`、`silent-boot generate entity`。
它们在应用里执行的是同一份代码，参数原样交过去。

## 目录

| 路径 | 放什么 |
|---|---|
| `migration/` | 迁移。`cargo run -- migrate generate "create order"` 新增一条 |
| `entity/` | 实体，由 `cargo run --features codegen -- generate entity` 从库结构生成，不手改 |
| `src/dto/` | 入参、出参。入参做校验，不出现实体类型 |
| `src/services/` | 业务逻辑。入参是 dto，错误是 `AppError`，审计字段在这里写 |
| `src/api/` | 接口。取参数、调 service、返回包络；新模块在 `src/api/mod.rs` 里挂上 |
| `config/app.toml` | 配置。环境变量 `APP__` 前缀可以覆盖任意一项 |
| `web/dist/` | 前端构建产物，编进二进制；没命中的路径回退到 `index.html` |

## 加一个功能

1. `cargo run -- migrate generate "create order"`，在生成的文件里写建表语句，然后 `cargo run -- migrate up`；
2. `cargo run --features codegen -- generate entity`；
3. `cargo run --features codegen -- generate module --table <表>`：按表生成 dto、service、api 并自动登记，
   之后照着 `article` 按需修改；生成的代码归你所有，再生成不会覆盖（要覆盖加 `--force`）。

规则见 `AGENTS.md`。
