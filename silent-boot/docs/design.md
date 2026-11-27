# silent-boot · 技术方案

> 状态：v0.8 ｜ 对应 [PLAN.md](../PLAN.md) ｜ 验证记录：[m0-verification.md](./m0-verification.md)、[m1-verification.md](./m1-verification.md) ｜ 编写日期：2026-11-25
>
> v0.2 变更：§6 由"SeaORM 对接"重写为"数据库与迁移"，迁移成为数据库部分的核心。
> v0.3 变更：sea-orm-cli 的能力包入框架的命令行（§6.4、§13）；迁移与实体生成改为显式命令，`dev` 只做运行循环。
> v0.4 变更：实体生成的目标位置改为固定约定，框架依赖的生成项不再提供参数（§6.4）；审计行为改由生成器写入实体文件末尾的 `ActiveModelBehavior` 实现。
> v0.5 变更：`generate entity` 以一直在用的 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 为基准命令，默认行为与它一致；唯一的差别是每个实体文件末尾的 `ActiveModelBehavior` 实现。
> v0.6 变更：澄清 SeaORM 不会自动填充审计时间，这两列仍由迁移定义、值由 service 层写入；实体文件不再追加任何内容，`generate entity` 的输出与基准命令一致（D13 的建议随之调整）。
> v0.7 变更：立项开发。补上实测发现 F1：提取器拒绝的响应不经过 `ExceptionHandler`，统一包络要另外覆盖（§5、§15 的 U7）。
> v0.8 变更：M1 落地。按实现修正了 §3（入口 `run`、`App` 的装配方式）、§5（`AppResult` 的定义、`TraceId` 与 `ErrorHandler` 两个中间件）、
> §6.7（对账方式和执行锁的释放）、§6.8（`migrate lint` 的规则）、§12（`dev --migrate` 与脚本注入）、§13（命令一览）；
> §15 增加 U8、U9；新增 §17 记录 M1 的落地情况和与设计不一致的地方。
>
> 标注"已核对"的内容来自本机源码、CLI 输出或 crates.io 检索；标注"待验证"的内容在 M0 spike 中确认（§16）。代码片段用于说明分层与接口形态，不是最终 API。

## 1. 总体架构

```
浏览器（Vue 3 SPA，生产环境内嵌于二进制）
        │ HTTPS
        ▼
负载均衡（Nginx / Caddy / K8s Service）
        │
        ▼
app 副本 ×N（silent-boot 应用，单二进制）
│
│  中间件链（启动器按序装配）
│  RequestId → Logger → CORS → Compression → Timeout
│  → ExceptionHandler → Auth（令牌 → 当前用户）→ 权限校验
│
│  /api/*    api ──▶ service ──▶ entity（SeaORM）
│                      │
│                      └ dto（入参、出参、校验、映射）
│
│  基础模块  migration · rbac · config（配置中心）
│            scheduler · storage · health · openapi
│  AppState  AppConfig(arc-swap) · DbRegistry · AuthManager · ConfigCenter
│
├──▶ 共享存储：SQLite / MySQL / PostgreSQL（结构由迁移管理）
├──▶ 配置中心：内置(DB) / etcd（可选）
└──▶ 对象存储：本地卷 / S3（可选）

命令行（开发期）：migrate · generate entity · generate module · dev · check · new
```

路径约定：

| 前缀 | 用途 | 认证 |
|---|---|---|
| `/api/*` | 业务与系统接口，JSON 包络 | 需登录，白名单除外 |
| `/swagger`、`/redoc` | OpenAPI 文档 | 按环境配置 |
| `/healthz`、`/readyz`、`/metrics` | 运维端点 | 不走认证 |
| `/__dev__/*` | 仅 dev 构建存在：事件流、错误覆盖层脚本 | 仅本机 |
| 其余 | 前端静态资源，未命中回退 `index.html` | 公开 |

## 2. 工程结构

### 2.1 框架仓库

```
silent-boot/
├── Cargo.toml                 # workspace
├── crates/
│   ├── silent-boot/           # 核心：App 启动器、迁移、配置、Db、认证与 RBAC、统一响应与错误
│   │                          # features: sqlite(默认) mysql postgres cli(默认) rbac config-center redis scheduler storage-s3
│   └── silent-boot-cli/       # 全局工具 `silent-boot`：new、dev 运行循环、转发应用内子命令
├── templates/
│   ├── app/                   # 脚手架模板（tera），随全局工具内嵌
│   └── module/                # 模块生成模板，编译进 silent-boot crate 的 cli feature
├── web/                       # 管理前端源码（Vue 3）
├── deploy/                    # Dockerfile、compose.yaml、k8s/
├── examples/                  # 由脚手架生成并纳入回归的示例应用
├── docs/
├── Makefile                   # build / dev / check / clean
├── PLAN.md
└── TODO.md
```

起步阶段只拆两个 crate（plan.md §6）。拆分触发条件：核心 crate 出现必须隔离的重依赖（如 etcd、Nacos 客户端）导致默认编译时间明显增加。

### 2.2 生成的应用

沿用 silent-template 的分层命名，先单 crate 多模块，业务规模变大再拆 crate：

```
my-app/
├── Cargo.toml
├── Makefile                   # dev / migrate / entity / check
├── silent-boot.toml           # 代码生成配置（表清单与选项）
├── config/app.toml            # 应用配置（可选，环境变量可完全替代）
├── migration/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # 应用的 Migrator（受管区块登记迁移）
│       ├── m20261125_120000_create_article.rs
│       └── fixtures/          # 开发夹具（仅 dev 与 test）
├── entity/src/                # 由 generate entity 从库结构生成，不手改
├── src/
│   ├── main.rs                # 入口：先分派子命令，否则启动服务
│   ├── app.rs                 # App::new() 的装配：配置、数据库、中间件、路由
│   ├── api/                   # 路由 + handler（薄）
│   │   ├── mod.rs             #   路由注册，含受管区块
│   │   └── article.rs
│   ├── dto/                   # 入参、出参
│   └── services/              # 业务逻辑
└── web/                       # 前端源码（与后端同仓）
```

## 3. 应用启动器与约定配置

`main.rs` 先判断是不是命令行子命令，命中就执行并退出，没命中才启动服务：

```rust
#[tokio::main]
async fn main() {
    // 命中 migrate / generate / check 等子命令时执行后退出
    if silent_boot::cli::try_dispatch().await.is_none() {
        if let Err(e) = silent_boot::App::new().run().await {
            eprintln!("应用启动失败: {e}");
            std::process::exit(1);
        }
    }
}
```

`App` 的启动顺序固定：

1. 加载配置：内置默认值 → `config/app.{env}.toml` → 环境变量 → 远端配置中心（可选），反序列化为强类型并校验（§8）。
2. 初始化日志（tracing，级别可热调整）。
3. 连接数据库：按 `auto_apply` 决定是否执行迁移（默认不执行，§6.7），`ping`，注册到 `AppState`。
4. 装配中间件与系统路由（health、metrics、OpenAPI、认证）。
5. 挂载业务路由 `api::routes()`。
6. `Server::new().bind(addr).with_shutdown(graceful).serve(route)`。

`AppState` 通过 `Route::with_state` 注入，handler 用 `State<AppState>` 提取（已核对）：

```rust
#[derive(Clone)]
pub struct AppState {
    pub env: Env,
    pub config: Arc<ArcSwap<AppConfig>>,   // 热更新
    pub db: DbRegistry,                    // 默认连接 + 命名连接
    pub auth: AuthManager,                 // 令牌签发与校验、权限快照缓存
    pub config_center: ConfigCenter,       // 配置中心句柄（变更订阅）
    pub started_at: NaiveDateTime,
}
```

约定配置示例（`config/app.toml`）：

```toml
[app]
name = "my-app"
env = "development"            # development | test | production
base_path = ""                 # 部署在子路径时填写
trusted_proxies = []           # 可信代理网段；只有 TCP 对端属于可信代理时才采信转发头

[server]
listen = "0.0.0.0:8080"
graceful_shutdown = "30s"
request_timeout = "30s"
max_body_size = "10MiB"

[database]
url = "sqlite://data/app.db"   # mysql://… postgres://…
max_connections = 16
connect_timeout = "8s"

[database.migrate]
auto_apply = "never"           # never | always：应用启动时是否执行迁移；默认由 `migrate up` 命令执行
lock_timeout = "30s"

[auth]
token_ttl = "2h"
refresh_ttl = "7d"

[rbac]
super_admin_role = "super_admin"

[config_center]
provider = "none"              # none | builtin | etcd | nacos
poll_interval = "5s"
```

环境变量覆盖规则：前缀 `APP__`，双下划线分层，例如 `APP__DATABASE__URL`。密钥类配置只接受环境变量或以 `_FILE` 结尾的变量所指向的文件，不写进配置文件和日志。

## 4. 分层规范：entity / dto / service / api

| 层 | 位置 | 职责 | 约束 |
|---|---|---|---|
| entity | `entity/src/` | 从迁移执行后的库生成，与表一一对应 | 不手写业务；不手改；不直接序列化给前端 |
| dto | `src/dto/` | 入参（`CreateXxxDto`、`UpdateXxxDto`、`XxxQueryDto`）、出参（`XxxDto`）、分页（`PageQuery`、`PageResult`）；校验；`From<Model>` | 不出现 `ActiveModel`；对外字段 camelCase |
| service | `src/services/` | 业务逻辑、事务、数据范围过滤；入参为 `&AppState`（或 `&DatabaseConnection`）加 dto | 不依赖 `Request`、`Response`；错误用 `AppError` |
| api | `src/api/` | 路由注册；提取器取 dto，调用 service，返回响应；声明权限码 | 不写业务逻辑；不直接操作 entity |

以"文章"为例。Silent 的提取器写法与 `Result<T, SilentError>` 返回类型已核对；`CurrentUser` 作为自定义提取器参与元组属于待验证（§16 V3）。

```rust
// dto/article.rs
#[derive(Debug, Deserialize, Validate)]
pub struct CreateArticleDto {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    #[validate(length(min = 1))]
    pub content: String,
    pub status: ArticleStatus,
    pub category_id: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ArticleQueryDto {
    #[validate(range(min = 1))]
    pub page: Option<u64>,
    #[validate(range(min = 1, max = 100))]
    pub page_size: Option<u64>,
    pub keyword: Option<String>,
    pub status: Option<ArticleStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleDto {
    pub id: String,
    pub title: String,
    pub status: ArticleStatus,
    pub category_id: String,
    pub created_by: String,
    pub created_at: NaiveDateTime,
}

impl From<article::Model> for ArticleDto {
    // 字段映射；敏感字段不出现在出参里
}
```

```rust
// services/article.rs
pub struct ArticleService;

impl ArticleService {
    pub async fn page(
        &self,
        db: &DatabaseConnection,
        q: &ArticleQueryDto,
    ) -> AppResult<PageResult<ArticleDto>> {
        let mut select = article::Entity::find().filter(article::Column::DeletedAt.is_null());
        if let Some(kw) = q.keyword.as_deref().filter(|k| !k.is_empty()) {
            select = select.filter(article::Column::Title.contains(kw));
        }
        let paginator = select.order_by_desc(article::Column::CreatedAt).paginate(db, q.page_size());
        let total = paginator.num_items().await?;
        let items = paginator.fetch_page(q.page() - 1).await?;
        Ok(PageResult::new(
            q.page(),
            q.page_size(),
            total,
            items.into_iter().map(ArticleDto::from).collect(),
        ))
    }
}
```

```rust
// api/article.rs
pub fn route() -> Route {
    Route::new("articles")
        .hook(RequireAuth)   // 该节点及其子路由都必须登录
        .get(page)
        .post(create)
        .append(Route::new("<id>").get(detail).put(update).delete(remove))
}

async fn page(
    (State(app), user, Query(q)): (State<AppState>, CurrentUser, Query<ArticleQueryDto>),
) -> AppResult<PageResult<ArticleDto>> {
    user.require("article:list")?;
    Ok(ArticleService.page(&app.db, &q.validated()?).await?)
}
```

说明：

- `AppResult<T>` 是 `Result<T, SilentError>` 的别名。`AppError` 通过 `From` 转成 `SilentError`（§5），handler 签名不需要泛型错误类型。
- `Serialize` 类型可直接作为 handler 返回值（已核对 `IntoResponse` 的桥接实现）；需要 201 等状态码时用框架的包装类型（如 `Created<T>`）。
- 权限声明的首版形式是 handler 内的 `user.require("模块:资源:动作")?`，常量字符串便于 `silent-boot check` 机检。注册处包装（`perm("article:list", handler)`）与 `#[permission]` 属性宏作为 M0 的备选评估，三选一后保持全局一致。
- 规范机检（M3 的 `silent-boot check`）：dto 中出现 `ActiveModel` 报错；service 中出现 `silent::Request` 或 `Response` 报错；api 中出现 `Entity::` 报错；业务 handler 没有 `require` 声明报错（白名单除外）。

## 5. 统一响应、错误与 OpenAPI

响应包络（成功与失败同构）：

```json
{ "code": 200, "message": "success", "data": { "page": 1, "pageSize": 20, "total": 100, "items": [] }, "traceId": "…" }
{ "code": 40301, "message": "无权限：article:delete", "data": null, "traceId": "…" }
```

- HTTP 状态码始终有意义（200、201、400、401、403、404、409、422、500），`code` 是细分业务码，规则为 `http_status * 100 + 序号`。
- 成功响应由返回值直接序列化，不再使用 silent-template 里"中间件把 JSON 响应反序列化再包装"的 `ResultWrapper`。
- 失败响应统一在 `ExceptionHandler` 中生成（已核对签名：入参为 `Result<Response>` 与 `State`）：

```rust
ExceptionHandler::new(|result: Result<Response>, state: State| async move {
    match result {
        Ok(res) => Ok(res),
        Err(e) => Ok(AppError::render(e, &state)),   // 还原业务码与字段错误 → 包络 + traceId
    }
})
```

- 实测发现（M0，见 `m0-verification.md` 的 F1）：提取器失败时的拒绝响应不经过 `ExceptionHandler`，响应体是纯文本的消息，例如 400 `content-type is error`、422 的 serde 原始错误。框架要另外覆盖这一类错误：自己的提取器把 `Rejection` 定成能渲染包络的类型；Silent 内置提取器的拒绝，用一层中间件把 4xx、5xx 且不是 JSON 的响应体改写成包络。

错误模型：

```rust
pub enum AppError {
    Unauthorized,                // 401 / 40100
    Forbidden(String),           // 403 / 40301，携带权限码
    NotFound(String),            // 404 / 40400
    Validation(String),          // 422 / 42200，携带字段错误
    Conflict(String),            // 409 / 40900
    Business(u32, String),       // 自定义业务码
    Internal(anyhow::Error),     // 500 / 50000；详情写日志，对外不暴露
}
```

Silent handler 的错误类型固定为 `SilentError`（已核对 `HandlerWrapper` 的实现）。业务码与字段错误通过 `SilentError::business_error_obj(status, payload)` 以 JSON 载荷写入 `BusinessError.msg`（已核对：响应转换时 `msg` 为 JSON 则按 JSON 输出）。代价是一次 JSON 字符串往返并丢失原始错误类型，已列入对上游的建议（§15 U1）。

OpenAPI：

- 用 silent-openapi（utoipa 6）标注 dto 与 handler；`/swagger` 在开发环境默认开启，生产环境可关闭或只开内网。
- `silent-boot dev` 在 Rust 变更后导出 `openapi.json`，用 `openapi-typescript` 生成 `web/src/api/schema.d.ts`，前端请求函数引用同一份类型（工具链已在 M0 验证，见 `m0-verification.md` 的 F15：`openapi-typescript` 7 要求 `typescript` 5.x；真实文档接入后再对一次）。

## 6. 数据库与迁移

### 6.1 原则

1. **迁移是数据库结构的唯一事实来源。** 实体是从数据库派生的产物（由 `generate entity` 生成），模块代码又是从实体派生的。链路固定为 `migration → 数据库 → entity → dto / service / api / 前端页面`。结构变更一律先写迁移，不手改实体，也不先改库再补迁移。
2. **迁移即代码，随二进制发布。** 迁移编译进应用，迁移列表与二进制版本一一对应。执行迁移的命令是应用自己的子命令（`cargo run -- migrate up`）；sea-orm-cli 的做法是 `cargo run --manifest-path` 单独编译一次迁移 crate（已核对其实现），新框架不需要。
3. **框架迁移与应用迁移分开管理。** 系统表由框架自己的迁移负责，随框架版本升级自动带入，不混入应用的迁移历史。
4. **默认安全，只向前滚动。** 破坏性操作必须显式标记；生产环境不依赖回滚，出问题用补偿迁移修复；执行前后都有状态检查和审计。
5. **命令行是唯一入口。** 迁移的生成、执行和实体生成都是一条条明确的命令，`dev` 不代劳；每条命令都支持 `--json`，能直接进 CI 和脚本。

### 6.2 连接管理

`DbRegistry` 持有默认连接与命名连接；连接参数来自配置；`ping()` 用于就绪探针（已核对）。驱动由 cargo feature 选择：`sqlite`（默认）、`mysql`、`postgres`，并启用 `with-chrono`、`with-json`。

### 6.3 开发工作流

```
silent-boot migrate generate "create article"   # 1) 写迁移：生成文件并登记
silent-boot migrate up                          # 2) 执行迁移
silent-boot generate entity                     # 3) 按库结构重新生成实体
silent-boot dev                                 # 另开一个终端：构建、启动、监听、重启、刷新
```

- 前三条命令与 sea-orm-cli 的 `migrate generate`、`migrate up`、`generate entity` 一一对应，对照表见 §6.4。
- `dev` 只负责运行循环：监听到 `src/`、`entity/`、`Cargo.toml` 变化后增量编译、重启、刷新浏览器。它不执行迁移，也不生成实体。
- `dev` 启动时如果有待执行的迁移，只提示对应的命令；`dev --migrate` 可以让它在启动前执行一次 `migrate up`，默认关闭。
- 应用进程默认不执行迁移（§6.7），结构只由 `migrate` 命令改变。
- 没有安装全局工具时，命令写成 `cargo run -- migrate generate "create article"` 这样的形式；模板的 Makefile 提供同义目标（`make migrate`、`make entity`、`make dev`）。

### 6.4 命令行：包入 sea-orm-cli 的能力

**命令对照**

| sea-orm-cli | 框架命令 | 差异 |
|---|---|---|
| `migrate init` | `migrate init` | 生成的迁移文件骨架可以带审计字段 |
| `migrate generate "<名称>"` | `migrate generate "<名称>"` | 同名。文件名规则相同（`m%Y%m%d_%H%M%S_<名称>`，已核对）；支持 `--table` 从列定义生成建表骨架 |
| `migrate up -n N` | `migrate up --steps N` | 在应用内执行；先框架迁移后应用迁移；带执行锁和审计 |
| `migrate down -n N` | `migrate down --steps N` | 仅开发环境 |
| `migrate status` | `migrate status` | 框架与应用分组显示，支持 `--json` |
| `migrate fresh`、`refresh`、`reset` | 同名 | 仅开发环境 |
| `generate entity --with-serde both -v -o entity/src/ -l` | `generate entity` | 等价，另有跳过系统表的约定和 `--dry-run`，见下 |
| `DATABASE_URL` 环境变量 | `--database-url` 或应用配置 | 不强制依赖环境变量 |
| 无 | `migrate baseline`、`repair`、`lint`；`--db <name>` 多库；`--json` | 框架新增 |

**实现方式**

- 能力写成 `silent-boot` crate 里的库函数，应用二进制在入口处分派（§3）。`migrate` 子命令放在 `cli` feature 里，默认开启；`generate entity`、`generate module`、`check` 放在 `codegen` feature 里，默认不开，开发和 CI 用 `--features codegen` 打开。
- 需要读写应用代码的命令都在应用的依赖图里执行，也就是 `cargo run -- <子命令>`。原因有两个：迁移列表只存在于应用代码里；实体和模块的生成器必须与该应用使用的 SeaORM、框架版本一致。sea-orm-cli 自己也把 `sea-orm-codegen` 锁在同一版本（已核对其依赖声明），版本错开时生成的实体可能编不过。
- 全局工具 `silent-boot` 很薄，只做两件事：`new`（脚手架模板随工具内嵌，工具与框架同步发版）；在应用目录里把其余子命令转发为 `cargo run --quiet -- <子命令>`。
- `dev` 的运行循环放在全局工具里，不能是应用的子命令：它要反复重建并重启应用，不能和被重启的服务是同一个可执行文件（Windows 上无法覆盖正在运行的 exe）。事件服务器也放在工具进程里，所以重启应用时页面连着的事件流不会断。
- M0 实测了 feature 的代价（`m0-verification.md` 的 F10）：开启 LTO 和 strip 后，`cli`（clap 一族）让示例二进制增加约 1.4 MiB，代码生成再增加约 2 MiB。所以把两者拆开：`cli` 默认开启，发布流程里的迁移 Job 要用它；`codegen` 默认不开，只有开发时生成实体才需要。
- 与 sea-orm-cli 的关系是"日常取代，保留后门"。应用的 `migration/` 仍是标准的 SeaORM 迁移 crate，sea-orm-cli 照样能操作；对照表没有覆盖到的选项（seaography、ER 图、前端格式实体等）继续用 sea-orm-cli。区别是 sea-orm-cli 不会执行框架迁移，也不带执行锁。

**`generate entity` 的固定约定**

基准命令是 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l`，`silent-boot generate entity` 不带参数时的行为与它一致。

规则只有一条：框架其它部分依赖的东西都是固定约定，不提供参数；只有纯偏好的东西才做成选项。

| 固定项 | 取值 | 谁依赖它 |
|---|---|---|
| 输出目录 | 应用根目录下的 `entity/src/`，`entity` 是独立的 crate | `generate module` 的模板、`check` 和分层规范都按这个路径引用实体 |
| 索引文件 | `lib.rs` | 同上 |
| 应用根目录 | `silent-boot.toml` 所在的目录；在子目录里执行命令时向上查找 | 所有读写应用代码的命令 |
| 时间类型 | chrono | DTO 模板和审计字段用 `NaiveDateTime` |
| serde | 派生 `Serialize` 与 `Deserialize` | 约定值，保证所有实体一致 |
| 跳过的表 | 两张迁移跟踪表和 `sys_*` 系统表 | 系统表的实体由框架 crate 提供，应用里重复生成会出现两套类型 |
| 输出 | 默认显示 sea-orm-cli `-v` 里的信息行：连接、发现过程、每个表的列、每个写入的文件；不输出 sqlx 的调试日志 | 生成结果要能直接看到；脚本用 `--json` |

- 可选参数首版只有四个：`--tables`、`--ignore-tables`、`--database-schema`、`--dry-run`。
- 实体格式、额外派生这类纯偏好的项，需要调整时直接用 sea-orm-cli。
- 首版只生成默认库的实体。命名连接的实体约定放在 `entity/src/<name>/`，M3 起支持。
- 实体文件按约定不手改，所以每次生成都整文件覆盖；`--dry-run` 只列出会变化的文件。

**与基准命令的输出一致**

- 对同样的表，生成的实体文件与基准命令的输出一致，框架不在实体文件里追加任何内容。每个文件末尾那一行 `impl ActiveModelBehavior for ActiveModel {}` 原样保留（基准命令的输出已对临时 SQLite 库实测）。
- 文件里只有 `Model`、`Relation` 和这一行实现，没有显式的 `ActiveModel` 结构体。SeaORM 2.0.4 没有 `ActiveModelBehavior` 的兜底实现，每个实体都要显式写一份（已核对），所以这一行不能删。
- 审计时间不是 SeaORM 自带的能力，框架也不在实体里处理它，见 §6.11。

### 6.5 迁移的分层

| 类别 | 归属 | 跟踪表 | 例子 |
|---|---|---|---|
| 框架结构迁移 | `silent-boot` crate 内置，随框架版本发布 | `sb_migrations` | `sys_user`、`sys_role` 等系统表 |
| 框架数据补丁 | 同上，写成幂等的 upsert | 同上 | 新版本新增的菜单、字典、默认参数 |
| 应用结构迁移 | 应用的 `migration/` crate | `seaql_migrations`（SeaORM 默认） | `article`、`order` 等业务表 |
| 开发夹具 | 应用的 `migration/src/fixtures/`，仅 dev 与 test | 不跟踪，可重复执行 | 测试账号、样例数据 |

- 跟踪表能分开，是因为 `MigratorTrait::migration_table_name()` 允许覆盖（已核对）。
- 执行顺序固定：先框架迁移，后应用迁移，两套迁移共用一把执行锁。
- 系统表与业务表之间不建外键，避免两套迁移互相牵制。

### 6.6 编写方式

- 默认用 Rust DSL（sea-query 的 `Table::create()`、`Table::alter()`），一份代码跑三种数据库。
- 需要原生 SQL 时通过 `manager.get_connection().execute_unprepared(...)` 执行，按 `manager.get_database_backend()` 分方言，方言差异集中到框架提供的少量 helper 里。
- 框架只在 SQLite 上自动验证迁移，其它数据库由 SeaORM 的方言支持负责（D14）。各库的能力差异决定了迁移的写法，写迁移时对照下表：

| 操作 | SQLite | MySQL 8 | PostgreSQL |
|---|---|---|---|
| 加列 | 支持 | 支持，多数可即时完成 | 支持 |
| 改列类型或约束 | 受限：要重建表。sea-query 生成这类语句时直接 panic，不是返回错误（M0 实测） | 支持，可能重建表 | 支持 |
| 删列 | 3.35.0 起支持 | 支持 | 支持 |
| 重命名表或列 | 支持 | 支持 | 支持 |
| DDL 能否放进事务 | 能 | 不能，隐式提交 | 能 |
| 不锁表建索引 | 不适用 | `ALGORITHM=INPLACE, LOCK=NONE` | `CONCURRENTLY`，且不能在事务内 |

### 6.7 执行模型

| 环境 | 执行时机 | 方式 |
|---|---|---|
| development | 开发者显式执行 | `silent-boot migrate up`（或 `cargo run -- migrate up`）；`dev --migrate` 可选 |
| test | 建测试库时 | CI 脚本或测试夹具调用同一个 `migrate up`；库多时改用结构快照恢复（优化项） |
| production（默认） | 发布流程中的独立步骤 | 以同一镜像执行 `migrate up`（CI 步骤或 K8s Job / initContainer），先迁移后滚动发布 |
| production（可选） | 应用启动时 | 单实例的小工具可以设置 `auto_apply = "always"`，启动时持锁执行 |

- 应用进程默认不执行迁移。这样"谁改了结构"永远只有一个答案：`migrate` 命令。
- **执行器**：框架的 `MigrationRunner` 依次完成：获取执行锁 → 框架迁移 → 应用迁移 → 写审计 → 释放锁。
- **并发**：sea-orm-migration 的 `exec_up` 自身不加锁（已核对，M0 也实测到不加锁会重复执行），多副本同时执行会重复跑。锁是 `sb_locks` 表里的租约行，三种数据库的写法相同（`db::lock`，M0 已实现，之后的定时任务也用它）。拿不到锁的副本等待超时后，只有在确认没有待执行迁移时才允许继续启动。
- **原子性**：SeaORM 只在 PostgreSQL 上默认把每条迁移包进事务，MySQL 与 SQLite 默认不包（已核对 `use_transaction` 的约定）。框架的 `#[migration]` 声明宏默认设置 `use_transaction() = Some(true)`：SQLite 上的迁移因此具备原子性，MySQL 上的数据回填部分可以回滚（DDL 仍是隐式提交）。个别迁移可以显式改回 `Some(false)`，例如 PostgreSQL 的 `CONCURRENTLY` 建索引。
- **失败恢复**：
  - PostgreSQL 与 SQLite：单条迁移失败整体回滚，修正后重跑。
  - MySQL：DDL 隐式提交，失败时可能只做了一半。约定一条迁移只做一件事；`migrate status` 定位失败点，人工修复后用 `migrate repair` 重跑或标记。
  - 每次执行的结果（含失败现场）写入审计表 `sb_migration_log`。
- **回滚**：`down` 默认不实现（SeaORM 的默认实现直接返回错误，已核对）。`down`、`fresh`、`refresh`、`reset` 只在开发环境可用；`fresh` 会清空整个库，生产环境拒绝执行。

### 6.8 安全护栏

- `silent-boot migrate status` 列出已执行、待执行、耗时；`/readyz` 用只读查询判断是否有待执行的迁移（`get_pending_migrations_read_only`，已核对），有则未就绪。
- `silent-boot migrate lint` 检查本次新增的迁移：出现 `drop_table`、`drop_column`、`rename_table`、`rename_column`、`truncate` 或改类型时，迁移必须带 `// silent-boot: destructive` 标记，否则 CI 失败。
- 破坏性变更分两次发布：先加新结构并双写（两个版本都能跑），下一个版本再删旧结构，与 §9.3 的滚动发布约束配合。
- 大表变更提供模板：锁超时设置、按主键范围分批回填、各库的不锁表建索引写法。
- SeaORM 的迁移记录只有版本名和执行时间，没有校验和（已核对）。框架用审计表补上执行实例与耗时，用 `migrate status` 提示已执行迁移的文件被修改过。

### 6.9 多数据源与基线

- 命名连接各自有迁移目录和跟踪表：默认库用 `migration/`，命名库用 `migration/src/<name>/`；执行时用 `--db <name>` 指定。
- 接入已有数据库：`silent-boot migrate baseline` 把当前库对应的迁移标记为已执行而不实际运行，再用 `generate entity` 反向生成实体，之后的新变更才走迁移。

### 6.10 与生成器的关系

- `generate module` 读的是迁移执行之后的库，执行前会确认没有待执行的迁移。
- 新表的路径是：`migrate generate --table` 生成迁移 → `migrate up` → `generate entity` → `generate module` 生成全栈模块。
- "比较实体与库的差异、自动生成 ALTER 迁移"在 SeaORM 2.0.4 里没有：`Schema::sync` 只新增，不修改也不删除，并标注为实验特性（已核对）。自动差异迁移作为 M3 之后的增强项单独评估。

### 6.11 实体与 ORM 约定

| 能力 | 设计 |
|---|---|
| 实体生成 | `silent-boot generate entity`（§6.4），固定输出到 `entity/src/`，文件内容与基准命令一致；底层与 sea-orm-cli 相同，调用 `sea-orm-codegen` 与 `sea-schema`（已核对） |
| 审计字段 | 统一约定 `id`（scru128 字符串）、`created_at`、`updated_at`、`created_by`、`updated_by`、`deleted_at`（可空）；建表迁移的骨架自带这些列，不加数据库默认值；四个审计字段都由 service 层写入（见下） |
| 软删除 | 约定按 `deleted_at` 过滤；生成器在查询模板里默认追加 `deleted_at IS NULL`；框架提供 `not_deleted()` 组合条件 |
| 分页 | `PageQuery { page, page_size }`（从 1 起，默认 20，上限 100）→ `PageResult<T>`；基于 `paginate`、`num_items`、`fetch_page`（2.0.4 已核对） |
| 事务 | service 层使用 `DatabaseTransaction`；跨 service 调用显式传递 `&DatabaseTransaction`，不做隐式事务传播 |
| 开发与测试 | SQLite 内存库用于集成测试；`Schema::sync` 只用于测试或原型快速建表，不替代迁移 |
| 多租户预留 | 审计字段之外预留可空的 `tenant_id`，不做隔离逻辑 |

**关于审计时间**

- SeaORM 2.0.4 不会自动填充 `created_at`、`updated_at`：核心里没有相关逻辑，`ActiveModelBehavior` 的钩子默认什么都不做（已核对）。
- 能借用的只有两样。一是 `before_save` 这类钩子，它只是机制，填充逻辑要自己写；钩子只挂在 `insert`、`update`、`save` 上（已核对注释），批量更新不会经过它。二是迁移里的 `date_time_default_now()` 和 `timestamp_default_now()`（2.0.2 新增），它们给列加数据库默认值，只在插入时由数据库按数据库时区取值，更新时不会动 `updated_at`。
- 所以这两列和原来一样，仍然要在迁移里定义，值由写入代码设置，silent-oss 现在就是这么做的。迁移骨架不给这两列加数据库默认值，因为数据库默认值取的是数据库时区，和本地时间的约定不一致。
- 框架做的只有一件事：生成的 create、update 模板把四个审计字段一起写好，批量更新也会更新 `updated_at`。手写的写入代码需要自己设置。
- 钩子方案作为 D13 的备选保留，默认不采用：它覆盖不到批量更新，手写的保存前逻辑也会和审计逻辑挤在同一处。

SeaORM 2.0 自带 `rbac` feature（`load_rbac`、`restricted_for`、`RbacEngine::user_can`，已核对），定位是表级与行级的查询审计。框架的功能权限走 §7；将来若需要"按记录授权"再评估接入，避免两套权限模型并存。

对 SeaORM 的反馈点（框架先行兜底，不 fork）：迁移执行器不带锁；迁移记录没有校验和；SQLite 默认不把迁移包进事务；没有差异迁移生成。

## 7. 认证与 RBAC

### 7.1 数据模型（前缀 `sys_`，ID 为 scru128 字符串）

| 表 | 关键字段 | 说明 |
|---|---|---|
| `sys_user` | `dept_id`、`username`、`nickname`、`email`、`phone`、`password_hash`、`status`、`last_login_at` 与审计字段 | 密码用 argon2（Silent `security` 的 `make_password`、`verify_password`，已核对） |
| `sys_role` | `name`、`code`、`sort`、`status`、`data_scope` | `data_scope`：1 全部、2 自定义、3 本部门、4 本部门及以下、5 仅本人 |
| `sys_menu` | `parent_id`、`name`、`menu_type`、`path`、`component`、`permission`、`icon`、`sort`、`visible`、`status` | `menu_type`：M 目录、C 菜单、F 按钮；`permission` 形如 `system:user:list` |
| `sys_user_role`、`sys_role_menu`、`sys_role_dept` | 关联表 | `sys_role_dept` 对应"自定义数据范围" |
| `sys_dept` | `parent_id`、`name`、`sort`、`status` | 部门树 |
| `sys_dict_type`、`sys_dict_data` | 字典 | 前端下拉与表格翻译 |
| `sys_config` | `config_key`、`config_value`、`is_builtin` | 系统参数，纳入配置中心的统一 API（§8.4） |
| `sys_operation_log`、`sys_login_log` | 操作与登录审计 | 含 `trace_id`、IP、UA、结果 |
| `sys_job`、`sys_job_log` | `name`、`cron`、`handler`、`mutex`、`status` 与执行记录 | 定时任务定义与执行历史 |
| `sys_user_token` | `token_hash`、`user_id`、`kind`、`expires_at`、`revoked_at`、`ip`、`ua` | 不透明令牌；启用 Redis feature 时改存 Redis |

系统表全部由框架迁移创建和升级（§6.5），业务表不与系统表建外键。系统表的实体由框架 crate 提供，`generate entity` 默认跳过它们。

### 7.2 认证

1. 登录：`POST /api/auth/login`（用户名与密码，可选验证码）；连续失败达到配置次数后锁定一段时间。
2. 签发：`access_token`（32 字节随机，服务端只存 SHA-256 哈希，TTL 2 小时，滑动续期）与 `refresh_token`（TTL 7 天，使用即轮换，旧令牌失效）。
3. 携带：`Authorization: Bearer <token>`；浏览器端同时支持 `HttpOnly` Cookie（生产环境 `Secure`、`SameSite=Lax`）。
4. 登出与强制下线：写 `revoked_at` 并同步失效缓存；"在线用户"页可查看与踢出。
5. `Auth` 中间件：解析令牌，查缓存或库，把 `CurrentUser` 写入请求扩展；白名单路由（登录、健康检查、OpenAPI、静态资源）不经过。
6. JWT 作为可选 feature，用于对外的 Open API；管理后台默认不用，因为撤销困难。

### 7.3 鉴权

- 声明位置在 handler（§4），判断 `CurrentUser.perms` 是否包含权限码；超级管理员角色拥有 `*:*:*`。
- 默认拒绝：`/api/*` 下未列入白名单的路由必须经过 `Auth`；业务 handler 没有 `require` 声明会被 `silent-boot check` 报错。
- 返回语义：未登录 401，已登录但无权限 403。
- 数据范围：`CurrentUser.data_scope` 换算为部门集合，service 层对需要数据范围的查询追加 `dept_id IN (…)`。

### 7.4 多节点缓存一致性

- 每个节点缓存"用户到角色与权限集合"、"令牌到用户"、"角色与菜单快照"。
- 失效机制：角色、菜单、用户授权变更时递增 `rbac_version`（单行表）；节点按配置间隔比对版本号（默认 2 秒），版本变化即整体重建快照；有 Redis 时通过 pub/sub 即时通知。
- 目标：变更后 5 秒内在所有副本生效（M2 验收）。

## 8. 配置中心

### 8.1 分层与优先级（从低到高）

1. 内置默认值（编译进二进制的 `default.toml`）。
2. 应用配置文件 `config/app.{env}.toml`。
3. 环境变量 `APP__SECTION__KEY`（以及 `_FILE` 指向的文件）。
4. 远端配置中心（可选）。

合并结果必须能反序列化为强类型 `AppConfig` 并通过校验，否则启动失败并列出原因。

### 8.2 Provider 抽象

```rust
#[async_trait]
pub trait ConfigProvider: Send + Sync {
    async fn version(&self) -> AppResult<u64>;               // 轻量轮询
    async fn fetch(&self) -> AppResult<ConfigSnapshot>;      // 版本变化时拉全量
    async fn watch(&self, events: Sender<ConfigEvent>) -> AppResult<()>;   // 可选的推送通道
}

pub struct ConfigSnapshot {
    pub namespace: String,
    pub env: String,
    pub version: u64,
    pub format: ConfigFormat,       // toml | json | yaml
    pub content: String,
    pub checksum: String,
    pub released_at: NaiveDateTime,
}
```

| Provider | 说明 | 优先级 |
|---|---|---|
| `builtin` | 快照存数据库，带版本号、发布记录与回滚；零外部依赖 | 首选 |
| `etcd` | `etcd-client`（官方客户端），watch 机制 | 次选 |
| `nacos` | `nacos-sdk`，供已有 Nacos 的环境使用 | 可选 feature，M0 评估成熟度 |
| `file` | 监听本地配置文件（dev） | 内置 |

没有接配置中心时，`provider = "none"`，配置只来自文件与环境变量，K8s 场景由 ConfigMap 与 Secret 提供。

### 8.3 热更新流程

```
变更到达（轮询发现版本变化、watch 推送或文件监听）
  → fetch → 反序列化 + 校验
      ├─ 失败：保留旧配置，记 WARN 日志，指标 +1，管理页显示"最近一次发布失败"
      └─ 成功：checksum 相同则忽略；不同则
            1. 写本地磁盘缓存（远端不可达时可启动）
            2. config.store(Arc::new(new_cfg))                  // ArcSwap 原子替换
            3. ConfigCenter.publish(ConfigChanged { keys })     // broadcast
            4. 订阅者按需响应：日志级别、限流阈值、功能开关、连接池参数…
```

不可热更新的项（监听地址、工作线程数等）集中登记在常量表中；这些键发生变化时给出"需重启生效"的明确提示。

### 8.4 管理界面

"系统管理 → 参数设置"（`sys_config`，业务键值）与"配置中心 → 配置发布"（整份配置快照）共用同一套版本与审计能力：查看差异、发布、回滚、查看各副本当前生效的版本。

## 9. 分布式部署

### 9.1 设计约束

| 状态 | 存放位置 | 说明 |
|---|---|---|
| 登录态 | 数据库或 Redis | 任意副本可校验 |
| 数据库结构 | 迁移（先执行，后发布） | 任意副本启动时结构都已就绪 |
| 上传文件 | `Storage` 抽象：本地卷（多副本需共享）或 S3 兼容存储 | 单机默认本地，多副本切 S3 |
| 定时任务 | `sys_job` 加互斥锁 | 多副本只执行一次 |
| 缓存 | 各节点内存加版本号失效 | 见 §7.4、§8 |
| 配置 | 配置中心或环境变量 | 所有副本一致 |
| 日志 | stdout（JSON） | 由平台收集 |

### 9.2 任务互斥

- 任务到点后先获取执行锁；锁记录 `job_id`、`owner`（实例 ID）、`lease_until`，获取使用一条原子条件更新，执行期间定时续租，结束释放。
- 互斥策略可选：`cluster`（全局一次）、`per_node`（每节点一次）。
- 不依赖 Silent `scheduler` 的存储实现（其 `Storage` trait 是模块私有且只有 MySQL 实现，已核对），互斥在框架层实现。

### 9.3 发布与可用性

- `/healthz` 表示进程存活；`/readyz` 要求数据库 `ping` 成功且没有待执行的迁移。
- 收到 `SIGTERM`：先置为未就绪（负载均衡摘除）→ `with_shutdown(graceful)` 排空在途请求（Silent 已支持，已核对）→ 停止调度并等待运行中的任务或超时。
- 滚动发布顺序：执行迁移（先框架后应用）→ 逐副本替换。要求相邻两个版本对同一数据库结构都可运行，破坏性变更分两次发布（§6.8）。
- 实例标识取 `INSTANCE_ID` 环境变量（默认主机名），写入日志与指标标签。

### 9.4 部署产物

- `Dockerfile`：三阶段，前端 `yarn build` → Rust musl 静态编译（依赖层缓存）→ `distroless/static`、非 root；版本取自 workspace `Cargo.toml`。
- `deploy/compose.yaml`：`app`（可 `--scale app=N`）、`db`、`caddy`（反向代理与 HTTPS）、可选 `redis`；健康检查用 `healthcheck` 子命令（distroless 镜像里没有 curl）。
- `deploy/k8s/`：Deployment、Service、Ingress、ConfigMap 与 Secret、HPA、PodDisruptionBudget、探针；迁移用独立的 Job（或 initContainer）以同一镜像执行 `migrate up`，在 Deployment 滚动之前完成；Helm chart 视需要。
- GitHub Actions：CI（fmt、clippy、nextest、前端 lint 与 build）；Release（多平台二进制与镜像）。

### 9.5 已知限制（写入文档）

- Silent 的限流是单机令牌桶，多副本下总额约为单机限额乘以副本数；全局限流需要 Redis 方案（后置）。
- WebSocket 与 SSE 跨副本广播需要粘性会话或 pub/sub，首版只保证单副本场景。
- SQLite 不支持多副本共享写入，只用于单机或开发。

## 10. 代码生成器

### 10.1 输入

```bash
# 1) 写迁移并执行
silent-boot migrate generate "create article"
silent-boot migrate up

# 2) 生成实体
silent-boot generate entity

# 3) 生成全栈模块
silent-boot generate module --table article --package content
```

在应用内执行时写成 `cargo run -- <子命令>`。前两条与 sea-orm-cli 的对应命令等价（§6.4）。

生成器读取数据库元数据（列名、类型、可空、默认值、注释、索引、唯一约束），读的是迁移执行之后的库，执行前会确认没有待执行的迁移。列注释只有 MySQL 能稳定读取（sea-schema 0.18.1 中 PostgreSQL 的 `comment` 字段被注释掉，SQLite 没有列注释，已核对），其余情况用命名规则加 `silent-boot.toml` 中的补充配置。`silent-boot.toml` 保存每张表的选项：包名、模块名、是否树形、查询条件、表单控件、字典映射、排除字段、页面类型。新表从零开始时，用 `silent-boot migrate generate --table article --col …` 生成建表迁移骨架（§6.4）。

### 10.2 输出

| 目标 | 内容 |
|---|---|
| `src/dto/article.rs` | Create、Update、Query、详情 DTO，枚举，`From<Model>` |
| `src/services/article.rs` | page、detail、create、update、remove（含批量）；create、update 写入四个审计字段 |
| `src/api/article.rs` | 路由、handler、权限声明、OpenAPI 注解 |
| `src/api/mod.rs` | 在受管区块内登记 `mod` 与 `route()` |
| `sql/menu/article.sql` | 菜单与按钮权限记录（含权限码） |
| `web/src/views/content/Article/` | 列表页（查询、表格、分页）与新增、编辑弹窗 |
| `web/src/api/content/article.ts` | 请求函数，类型来自 OpenAPI |

### 10.3 可重复生成

- 首版策略：目标文件已存在则跳过，`--force` 覆盖；`--dry-run` 预览；写入前确认工作区可回滚。
- 进阶策略：生成文件分为受管区块（`// <silent-boot:gen>` 与 `// </silent-boot:gen>` 之间，再次生成时覆盖）和自由区块（手写内容保留）。
- 模块模板编译进 `silent-boot` crate（tera），与应用使用的框架版本一致；应用可用 `--templates <dir>` 覆盖。
- 生成后自动执行 `cargo fmt` 与 `cargo check`，失败时不落盘。

### 10.4 在线生成（M6）

管理界面"开发工具 → 代码生成"：选表、编辑选项、预览、下载 zip；dev 模式下可以直接写入工作区，正在运行的 `dev` 会自动重新编译。它调用的是 `generate module` 背后的同一批库函数。

## 11. 管理前端

- 技术栈：Vue 3、Vite、TypeScript、shadcn-vue（Tailwind CSS）、TanStack Query、TanStack Table、Vue Router、Pinia、vue-i18n、lucide 图标、ECharts（按需）。
- 首批页面：登录、主布局（侧边菜单、顶栏、多页签）、首页、系统管理（用户、角色、菜单、部门、字典、参数、通知公告）、系统监控（在线用户、定时任务、数据库迁移、服务监控）、开发工具（代码生成、接口文档）。
- 路由与菜单由后端 `sys_menu` 驱动：登录后拉取菜单树，注册动态路由；按钮级权限用 `v-permission` 指令控制。
- 请求层：自动携带令牌，401 时用 refresh token 重试一次，统一错误提示，回显 `traceId` 便于排查。
- 构建与嵌入：`yarn build` 输出 `web/dist`，后端用 rust-embed 嵌入；dev 由 Vite 开发服务器代理 `/api`。
- 组件收敛在 `web/src/components/ui`，降低更换组件库的成本。

## 12. 开发体验与热加载

| 层级 | 能力 | 实现 | 阶段 |
|---|---|---|---|
| L0 提示 | 待执行迁移的提醒 | `dev` 启动时只读查询待执行的迁移，提示 `silent-boot migrate up`；`--migrate` 可选在启动前执行一次 | M1 |
| L1 进程级 | 改 Rust 后增量编译、重启、刷新浏览器 | 全局工具 `silent-boot dev`（M0 已实现，`crates/silent-boot-cli`）：`notify` 监听 `src/`、`entity/src/`、`migration/src/`、`templates/`、`Cargo.toml`（防抖 150 毫秒，系统的文件事件不可用时退回轮询）→ `cargo build`（旧进程接着运行）→ 构建成功才先 SIGTERM 再强杀旧进程 → 运行新产物的一份副本；构建没过就什么都不换，上一次的版本接着服务。事件服务器在工具进程里（默认 `127.0.0.1:9230`），页面引入它给出的 `reload.js` 后收到 `reload` 自动刷新，构建失败时把错误盖在页面上，并写明页面现在由上一次构建成功的版本提供服务 | M0 |
| L1 进程级 | 编译错误可视化 | 编译输出留在终端，并通过 SSE 推到浏览器覆盖层 | M1 |
| L1 进程级 | 前端热更新 | Vite HMR，代理 `/api` | M1 |
| L2 运行时 | 配置热更新 | §8.3 | M4 |
| L2 运行时 | 模板热更新 | 监听模板目录，调用 `Tera::full_reload()`（2.4.0 已核对存在，需 `glob_fs` feature）；Silent 的 `TemplateMiddleware` 只在启动时加载一次，重载由框架层处理 | M4 |
| L2 运行时 | 静态资源 | rust-embed 在 debug 构建下从磁盘读取 | M1 |
| L2 运行时 | 权限、字典、参数缓存 | 版本号失效 | M2 |
| L2 契约 | OpenAPI 到前端类型同步 | 导出 `openapi.json`，交给 `openapi-typescript` | M4 |
| L3 实验 | 代码热补丁（不重启进程、保留连接池） | dylib 重载或 `subsecond`（0.8.0-alpha.3） | M6 评估 |

L3 的前置条件：Silent 的路由树在启动时冻结（v2.15 起预构建为 `Arc<RouteTree>`），运行中替换路由需要上游提供可替换的服务句柄，因此 L3 只作技术验证，不进入首版承诺。

`dev` 自带监听循环，以保证"编译成功 → 重启 → 通知"的顺序可控；cargo-watch 的仓库已归档，不再沿用。`dev` 不执行迁移、不生成实体（§6.3）。

## 13. 命令行

### 13.1 入口

| 入口 | 前提 | 命令 |
|---|---|---|
| 应用内子命令 | 无，随应用编译；生成代码的子命令还要 `--features codegen` | `cargo run -- migrate up`、`cargo run --features codegen -- generate entity`、`cargo run --features codegen -- generate module`、`cargo run -- check` |
| 全局工具 | `cargo install silent-boot-cli` | `silent-boot new`、`silent-boot dev`；在应用目录里 `silent-boot migrate …`、`silent-boot generate …`（自动带上 `--features codegen`）、`silent-boot healthcheck …` 被转发为 `cargo run --quiet -- <子命令>`，`--help` 也交给应用；应用有多个可执行目标时在子命令前面写 `--bin <目标>` |
| Makefile | 模板自带 | `make dev`、`make migrate`、`make entity`、`make check` |

下文为简洁起见都写成 `silent-boot <子命令>`。

### 13.2 命令一览

```
silent-boot new <name> [--db sqlite|mysql|postgres] [--git]

silent-boot migrate init                                   # = sea-orm-cli migrate init
silent-boot migrate generate "<名称>" [--table <t> --col <名:类型>…]
silent-boot migrate up [--db <name>] [--steps <n>] [--dry-run]
silent-boot migrate down [--steps <n>]                     # 仅开发环境
silent-boot migrate status [--db <name>] [--json]
silent-boot migrate fresh | refresh | reset                # 仅开发环境
silent-boot migrate baseline [--db <name>]
silent-boot migrate repair
silent-boot migrate lint                                   # 破坏性操作标记检查

silent-boot generate entity [选项]                          # 等价于 sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l
silent-boot generate module --table <t> [--package <p>] [--force] [--dry-run]
silent-boot generate openapi-types

silent-boot dev [--bin <目标>] [--port <端口>] [--watch <目录或文件>,…]   # 其余选项随 M1 补上
silent-boot check                                          # 分层规范、权限声明、配置校验
<应用> healthcheck                                         # 容器内使用
```

- 所有命令支持 `--json` 输出，便于脚本和 AI 编码助手调用；模板自带 `AGENTS.md`。
- `examples/` 下保留由 `new` 生成的示例应用并纳入 CI，防止模板腐化。

## 14. 测试与质量门禁

- 单元测试：配置合并优先级、错误码映射、权限集合计算、生成器模板快照。
- 集成测试：`TestClient`（`silent` 的 `test` feature，已核对）加 SQLite 内存库或临时文件库；覆盖鉴权、CRUD、分页边界、校验失败。
- 迁移测试（只在 SQLite 上跑，D14）：
  - 从零建库；
  - 升级测试：保留每个已发布版本的迁移列表，"旧版本建库 → 写入数据 → 新版本 `migrate up` → 断言数据保留"；
  - 两进程并发 `migrate up`，只执行一次；
  - SQLite 上迁移中途失败不留下半成品；
  - 破坏性操作缺少标记时 lint 失败。
  - MySQL、PostgreSQL 上的行为不自动验证（D14），只在写迁移时对照 §6.6 的差异表。
- 命令行测试：`generate entity` 的结果与 sea-orm-cli 对比；`--dry-run` 不写文件；`--json` 输出可解析。
- 多副本测试：同一测试进程内启动两个 `App` 实例共享一个数据库文件，验证令牌互通、权限变更传播、任务只执行一次。
- 生成物测试：对示例库执行 `generate module`，对生成的项目跑 `cargo check` 与 `clippy -D warnings`；生成的 create、update 与批量更新正确写入审计字段。
- 门禁：`cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run`、`cargo deny check`、`migrate lint`、前端 lint 与 build；pre-commit 复用现有配置。

## 15. 对 Silent 核心的依赖与反馈

| # | 事项 | 现状（已核对） | 影响 | 处理 |
|---|---|---|---|---|
| U1 | 错误携带任意 payload | handler 错误类型为 `SilentError`；业务信息只能进 `BusinessError.msg` | 错误链路多一次 JSON 往返，丢失原始错误类型 | 框架先用 `business_error_obj`；向上游提议增加可 downcast 的 payload |
| U2 | 路由树运行时替换 | 启动时冻结 | 代码热补丁受限 | L3 实验的前置条件 |
| U3 | 静态资源 SPA 回退 | `StaticOptions` 只有压缩与目录列表 | 前端 history 路由刷新 404 | 框架用 `<path:**>` 兜底；可向上游提议 `fallback` 选项 |
| U4 | `session` 存储实现 | 基于 async-session，其传递依赖 bincode 已不再维护（RUSTSEC-2025-0141，目前在 `deny.toml` 中忽略） | 依赖不再维护 | 框架不启用，自管令牌 |
| U5 | 调度存储 | `Storage` trait 模块私有，只有 MySQL 实现 | 无法复用做分布式互斥 | 框架层实现 |
| U6 | 默认 feature 与文档不一致 | `Cargo.toml` 中 `default = ["server"]`，`test` 需显式启用；`AGENTS.md` 写的是默认启用 `server`、`test` | 文档误导 | 可顺带修正文档 |
| U7 | 提取器拒绝不经过 `ExceptionHandler` | 提取器失败时直接返回纯文本的拒绝响应，`ExceptionHandler` 看到的是 `Ok`（M0 实测，见 `m0-verification.md` 的 F1） | 统一错误包络覆盖不到参数错误 | 框架先自行兜底（§5）；向上游提议让拒绝以 `Err` 进入异常处理 |
| U8 | sea-orm-migration 的迁移器容不下"库比这个版本新" | `up` 和 `status` 发现跟踪表里有当前版本不认识的迁移就报错；执行循环 `exec_up_with` 没有公开（M1 实测，见 `m1-verification.md` 的 F16） | 滚动发布时旧副本执行不了迁移、就绪检查失败 | 框架自己算待执行的清单、自己写执行循环；向上游提议把"不认识的已执行迁移"当成信息而不是错误，并公开 `exec_up_with` |
| U9 | `Server` 没有可以编程触发的关停 | `serve` 只认 Ctrl-C 和 SIGTERM；测试用的关停通知是私有的（M1 读源码确认） | 嵌入、测试和 `dev` 工具没法让服务优雅地停下 | 启动器用 `select!` 丢掉 serve 的 future，不等在途请求；向上游提议公开一个关停句柄 |

## 16. 待验证事项（M0）

| # | 事项 | 验证方式 | 通过标准 |
|---|---|---|---|
| V1 | Silent + SeaORM 2.0 最小链路 | spike：`with_state` 注入连接、提取器 handler、`ExceptionHandler` | SQLite 内存库 CRUD 通过 TestClient |
| V2 | MSRV | `cargo +1.94 check` | 通过 |
| V3 | 自定义提取器进入元组 | 实现 `CurrentUser` 提取器，参与三元组 handler | 可编译并可取到值 |
| V4 | SPA 兜底与 `<path:**>` 优先级 | 注册静态资源与兜底路由 | `/articles/123` 返回 `index.html`，`/api/*` 不受影响，缺失资源仍为 404 |
| V5 | sea-orm-cli 2.0.4 | `migrate generate` 生成的文件与登记方式、默认 `ActiveModelBehavior` 实现 | 文件骨架可替换为框架模板；实体文件末尾保留那一行空实现 |
| V6 | 迁移并发 | 两进程同时 `migrate up`（PostgreSQL、MySQL、SQLite）：先不加锁，再加框架的执行锁 | 不加锁时问题复现；加锁后只执行一次 |
| V7 | rust-embed 前端嵌入 | 嵌入 `web/dist` | release 访问正常，debug 从磁盘读取 |
| V8 | Tera 重载 | `full_reload()` | 修改模板后无需重启 |
| V9 | 监听重启循环 | 全局工具的 `dev`：`notify` + `cargo build` + 重启 + SSE 通知 | 记录"保存到页面刷新"的耗时 |
| V10 | OpenAPI 到 TS 类型 | `openapi-typescript` | 生成 `schema.d.ts` 并被前端引用。M0 用手写的文档验证通过（F15） |
| V11 | 任务互斥 SQL | 双实例同时触发 | 只有一个实例执行 |
| V12 | 项目名占用 | crates.io 与 GitHub 检索 | 名称可用 |
| V13 | SQLite 上的迁移事务与 ALTER 边界 | `use_transaction() = Some(true)` 的迁移中途失败；删列、改类型、重命名 | 失败后库无变化；记录哪些操作需要重建表 |
| V14 | 应用内 `generate entity` | 用 `sea-orm-codegen` 与 `sea-schema` 的库级实现，在应用的依赖图里执行；与基准命令 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 输出到同一目录的结果对比 | 实体固定生成到 `entity/src/`；对同样的表，输出一致；应用的 SeaORM 版本与 codegen 版本一致 |
| V15 | 两套迁移器共用一个库 | 框架迁移表与应用迁移表并存，`migrate up` 顺序正确 | 首次建库建齐系统表；框架升级能带入新迁移 |
| V16 | MySQL 部分失败恢复 | 不做（D14） | — |
| V17 | `cli` feature 的代价 | 对比开关 `cli` feature 时 release 二进制的体积与编译时间；验证子命令命中后不启动服务 | 体积与时间差异记录在案，并据此决定是否改用独立的开发用 bin target |

## 17. M1 的落地情况和与设计不一致的地方

M1 的完成标准逐条对照见 `m1-verification.md`。这一节只记"实现和前文写的不一样"的地方，以前文的章节为准读的时候要看这里。

### 17.1 入口与装配（§3）

- 应用的 `main.rs` 只有一句 `silent_boot::run(migration::Migrator, app::serve).await`：`run` 先看第一个参数是不是框架的子命令，是就执行并退出，否则加载配置、初始化日志，再调用 `serve(config)`。
- `App` 不是 `App::new()`，而是 `App::with_config(config)`（或者 `App::builder()` 读当前目录的配置）拿到 `AppBuilder`，链上 `.migrator(..)`、`.route(..)`、`.openapi(doc)`、`.assets(assets)`，`build().await` 连库、装配路由并注入 `AppState`。测试用 `app.router()` 配合 `TestClient`，不监听端口。
- `AppState` 现在只有 `env`、`config`、`db`、`started_at` 和两组迁移器。`AuthManager`、`ConfigCenter` 随 M2、M4 加。
- 端口被占用时 `bind` 返回错误，不 panic（Silent 的 `Server::bind` 会 `expect`）。监听地址为 `0.0.0.0` 或 `[::]` 时日志里报的仍是配置里的写法。
- 配置文件是 `config/app.toml` 加 `config/app.<env>.toml`（环境取 `APP_ENV`，再取文件里的 `app.env`，默认 `development`），环境变量前缀 `APP__`。前文写的"远端配置中心"留到 M4。

### 17.2 响应与错误（§5）

- service 层的返回类型是 `AppResult<T>`，也就是 `Result<T, AppError>`；handler 的返回类型是 Silent 的 `Result<T>`，对 service 的结果直接用 `?`，`AppError` 会自动转成 `SilentError`。前文把 `AppResult` 定义成 `Result<T, SilentError>`，那样 service 里对 SeaORM 的错误没法用 `?`。
- 失败响应由框架自己的 `ErrorHandler` 中间件生成，不用 Silent 的 `ExceptionHandler`（它拿不到请求，补不了追踪号）。`TraceId` 中间件给每个请求发追踪号，写进响应头 `x-request-id` 和 tracing 的 span。
- 业务码按"HTTP 状态码 × 100 + 序号"：40100、40301、40400、40000、42200、40900、42900、50000；自定义业务码的前三位是 HTTP 状态码，不是 4xx、5xx 时按 400 返回。
- 字段错误在 `data` 里：`{"字段": [{"code": "规则", "message": "说明", "params": {..}}]}`，说明是一句中文，按规则生成。
- `handler` 里的 panic 不会断开连接：记日志，返回 500 的包络。

### 17.3 分页（§4、§6.11）

分页参数不放进入参 DTO，用单独的 `Page` 提取器取（见 F18）。`PageQuery` 仍然是那两个字段，`Page(pub PageQuery)`。

### 17.4 迁移（§6.7、§6.8）

- 待执行的迁移按"这个版本里有、跟踪表里还没有"来算；跟踪表里有、这个版本不认识的迁移不算问题（见 F16）。
  滚动发布时旧副本照常就绪、照常启动。一个和线上分叉的版本被拒绝执行。
- 执行循环由框架写：拿锁 → 对账 → 按清单执行（默认在事务里）→ 写跟踪表 → 写执行记录 → 显式释放锁（见 F17）。
- `/readyz` 只在有待执行的迁移、或者数据库连不上时才报未就绪。
- `migrate status` 多一类 `newer`：数据库里有、这个版本不认识的迁移。
- `migrate lint` 检查的是迁移文件的源码文本（去掉注释之后）：出现删表、删列、删索引、改名、清空、改列的写法要带
  `// silent-boot: destructive` 标记；另外检查 `migration/src/` 下的迁移文件和迁移器受管区块里的登记对得上。
  `--since <Git 引用>` 只看新增的文件（包括还没提交的）。
- `migrate baseline` 只处理应用的迁移：把待执行的迁移标记成已执行，不建表；框架的迁移照常由 `migrate up` 执行。
- `migrate repair --applied|--pending <版本> [--scope app|framework]` 把一条迁移标记成已执行或未执行。

### 17.5 命令行（§13）

已经有的：`migrate generate / up / status / lint / baseline / repair`、`generate entity`、`generate module --table`、`healthcheck`、
全局工具的 `new`、`dev`，以及把 `migrate`、`generate`、`healthcheck` 转发给应用（从当前目录往上找 `silent-boot.toml` 定位应用；应用的输出和退出码原样返回）。还没有的：`migrate down / fresh / refresh / reset`、`generate module` 之外的平台选项、`generate openapi-types`、`check`。
子命令的输出可以写到调用方给的缓冲区，`--json` 目前只有 `migrate status` 有。

数据库地址的优先级：`--database-url`、环境变量 `SB_DATABASE_URL`、配置文件里的 `[database].url`。
在子目录里执行命令时向上找 `silent-boot.toml`，把它所在的目录当作应用根目录。

### 17.6 dev（§12）

- `dev --migrate`：第一次构建成功之后先执行一次 `migrate up`，失败就不启动应用并告诉页面；默认只提示有几条迁移没执行。
  这两件事只对 silent-boot 应用做（目录里有 `silent-boot.toml`）。
- 应用进程带着 `SB_DEV_RELOAD_URL` 启动，开发构建的应用自动把页面脚本注入 HTML，不用手改页面。
- 重启的顺序是“先构建，成功才停旧进程”：构建没过时上一次的版本接着服务，终端和页面都写明了这一点（F29）。
  运行的是构建产物的副本（`target/debug/.silent-boot-dev/<bin>`）：构建期间旧进程还在跑，Windows 上运行中的 exe 不能被覆盖。
- 前端热更新（Vite）、配置和模板热更新还没做；编译错误的页面覆盖层已经有了。

### 17.7 代码生成（§10）

`generate module` 的第一步只做一件事：按一张表生成 dto、service、api 三个文件并登记，写法和示例模块一致。
不支持的情况会说明原因：主键不是"不自增的字符串"或"自增的 32 位整数"、有生成列、表名不是合法标识符。
生成的模块不处理多表关联、菜单、权限声明和前端页面，这些在 M3。接口路径是表名的复数（`article` 对应 `/api/articles`）。
