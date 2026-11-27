# silent-boot · 总体规划

> 状态：**已确认，M0 验证完成，M1 完成（草稿 PR #236，CI 通过）** ｜ 编写日期：2026-11-25
> 技术方案：[design.md](docs/design.md) ｜ 验证记录：[m0-verification.md](docs/m0-verification.md) ｜ 任务清单：[TODO.md](TODO.md)
>
> silent-boot 是基于 Silent 的全栈快速开发框架。它先放在 Silent 仓库里，以嵌套 workspace 的形式开发：目录 `silent-boot/`，有自己的 `Cargo.toml` 和锁文件，不进外层 workspace。是否拆成独立仓库，M1 之后再决定。
>
> v0.2 变更：数据库部分改为以迁移为核心，迁移是数据库结构的唯一事实来源。
> v0.3 变更：sea-orm-cli 的能力包入框架的命令行，迁移与实体生成改为显式命令，`dev` 只做运行循环。
> v0.4 变更：实体生成的目标位置改为固定约定，框架依赖的生成项不再提供参数。
> v0.5 变更：`generate entity` 以一直在用的 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 为基准命令，默认行为与它一致。
> v0.6 变更：澄清 SeaORM 不会自动填充审计时间，这两列仍由迁移定义、值由 service 层写入；`generate entity` 的输出与基准命令一致。
> v0.7 变更：立项开发。按"先在 Silent 仓库内以嵌套 workspace 开发"调整 D1 与 §6 第 1 条；任务清单移到 `TODO.md`，验证结论记在 `docs/m0-verification.md`。
> v0.8 变更：数据库能力全部交给 SeaORM（D14），框架只用 SQLite 做自动化测试；M0 的验证项全部有结论，转入 M1。
> v0.9 变更：M1 完成。启动器、配置、统一响应与错误、迁移子命令、`new`、`dev`、`generate module` 的第一步都落地，逐条对照在 `docs/m1-verification.md`；D15 记下滚动发布时对待执行迁移的定义。
> v0.10 变更：`dev` 在终端里是分栏面板（左边重启、执行迁移、回退迁移、看迁移状态，右边日志），`--plain` 换回直接打印；`migrate down --steps N` 落地（只回退应用迁移，只在开发环境可用）。M4 的“`dev` 增强”里浏览器刷新、错误覆盖层、面板已在 M1 之后做掉，剩下 OpenAPI 到 TS 类型同步。

## 1. 背景与定位

### 1.1 现状

| 资产 | 已具备 | 对"快速做出全栈后台"还缺什么 |
|---|---|---|
| Silent 2.16.2 | 路由、中间件、提取器、`State` 注入、`IntoResponse`、OpenAPI、TestClient、cron 调度、模板、静态资源、优雅关停、限流、指标 | 是 Web 框架而不是应用框架：没有启动约定、权限、配置中心、代码生成 |
| silent-template | `api / dto / services / entity / common / configs / migration` 分层、统一响应、分页、Casbin RBAC | 静态模板，复制后各自演化、没有升级通道；策略存文件；示例配置里密码用 MD5、JWT 密钥为固定值且有效期 10 天；单机；没有前端和生成器 |
| silent-oss 等项目 | SeaORM 2.0、迁移、多 crate 工程、Makefile、Compose、musl 镜像等工程实践 | 每个项目重复搭一遍 |

分层方式和工程实践已经稳定，缺的是把它们产品化：**框架 crate + 脚手架 + 生成器 + 管理前端 + 部署套件**。

### 1.2 定位

**Silent 生态的 Spring Boot + RuoYi**：约定优于配置的应用底座，内置 RBAC 管理后台，一条命令生成全栈模块，可多副本部署。

- 框架层（crate）：启动器、迁移、配置中心、认证与 RBAC、SeaORM 接入、统一响应与错误，随版本升级。
- 工具层（命令行）：`migrate`（包入 sea-orm-cli 的迁移能力）、`generate entity`、`generate module`、`dev` 运行循环、`check`、`new`。
- 应用层（生成到项目里的代码）：业务模块归应用所有，可以任意修改。
- 前端（Vue 3）：管理后台 SPA，生产环境内嵌进单个二进制。

### 1.3 目标用户与场景

- 自己的中小型业务系统、内部工具、管理后台：希望半天内得到一个带权限、连库、可上线的后台。
- Silent 生态开发者：熟悉 Rust，不想每个项目重复搭脚手架。

## 2. 目标与衡量指标

| # | 目标 | 衡量方式 |
|---|---|---|
| G1 | 极快起步 | `silent-boot new demo && silent-boot dev` 后可访问登录页；依赖缓存热身后不超过 5 分钟 |
| G2 | 极快加模块 | 一张表得到后端 CRUD、权限和前端页面：一条 `generate` 命令，10 分钟内完成联调 |
| G3 | 分层一致 | 生成的代码严格符合 entity / dto / service / api 分层，CI 可机检 |
| G4 | 反馈快 | 改 Rust 代码后增量编译、重启并刷新浏览器；改配置和模板不重启 |
| G5 | 可横向扩展 | `docker compose up --scale app=3` 直接可用：令牌共享、任务不重复执行、滚动重启无 5xx |
| G6 | 单二进制交付 | 前端内嵌；镜像 distroless、非 root |
| G7 | 结构变更可信 | `migrate up` 与 `generate entity` 两条命令完成结构变更；实体固定生成到 `entity/src/`，对同样的表输出与基准命令 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 一致；"旧版本升级"测试通过，多副本不会重复执行 |

## 3. 范围

### 3.1 能力范围

| 诉求 | 对应能力 | 里程碑 |
|---|---|---|
| 快速实现 Web 服务 | 启动器（约定配置 + 中间件装配）、脚手架 `new`、分层模板 | M1 |
| DTO / API / Service 模式 | 分层规范、DTO 校验与映射、统一响应与错误、OpenAPI | M1 |
| 数据库与迁移 | 以迁移为核心：迁移是结构的唯一事实来源；`migrate` 与 `generate entity` 包入 sea-orm-cli 的能力，作为应用的子命令执行；框架迁移与应用迁移分开；三种数据库兼容、并发与失败恢复；外加连接管理、审计字段、软删除、分页 | M1 |
| RBAC | 用户、角色、菜单（权限）、部门、字典、参数、日志，以及认证与权限中间件、管理前端 | M2 |
| 快速后台生成 | CLI 生成器（表到后端、前端、菜单，新表可生成迁移骨架）；在线生成 | M3 / M6 |
| 配置中心 | 分层配置、Provider 抽象、热更新、版本与回滚、管理页 | M4 |
| 辅助开发的热加载 | `dev` 运行循环、配置与模板热更新、实验性的代码热补丁 | M1 / M4 / M6 |
| 基础分布式部署 | 无状态多副本、迁移先行、共享存储、任务互斥、缓存失效、健康检查与优雅关停、Compose 与 K8s | 设计自 M1 贯穿，M5 验证 |

### 3.2 非目标（首版不做）

- 微服务治理：注册中心、服务网格、链路追踪后端。"分布式部署"仅指**同一应用多副本**水平扩展。
- 低代码与可视化设计器：表单设计器、流程引擎、大屏。
- 多套前端：只维护 Vue 3 一套，不做 SSR。
- 多租户：数据模型预留 `tenant_id`，不实现隔离逻辑。
- 运行时动态路由、动态语言级热更新：Rust 不具备条件；代码热补丁只作实验特性。
- 重复实现 Silent 已有能力：HTTP/2、HTTP/3、WebSocket、gRPC、TLS 直接由 Silent 提供。
- 重新实现 sea-orm-cli：只包入日常用到的部分，底层调用同一套库；其余选项继续用 sea-orm-cli。
- 自动差异迁移：SeaORM 2.0.4 没有比较实体与库结构并生成 ALTER 迁移的能力，M3 之后再评估。

## 4. 技术选型

| 领域 | 选型 | 依据与备注 |
|---|---|---|
| Web | Silent `2.16`（crates.io 版本，MSRV 1.85） | features：`server` `static` `security` `scheduler` `template` `tower-compat` `metrics`；测试用 `test` |
| ORM 与迁移 | SeaORM `2.0`、sea-orm-migration `2.0`；实体生成调用 `sea-orm-codegen` 与 `sea-schema`（sea-orm-cli 用的同一套库） | 2.0.0 发布于 2026-07-19；SQLx 0.9、SeaQuery 1.0；**rust-version 1.94**；`sea-orm-codegen` 须与 SeaORM 同版本（sea-orm-cli 用 `=` 锁版） |
| 数据库 | 交给 SeaORM：SQLite（默认），MySQL、PostgreSQL 通过 feature 选择 | 框架自己只用 SQLite 做自动化测试；其它数据库由 SeaORM 负责，框架只保证带对应 feature 时能编译（D14） |
| 命令行 | clap 4 | 全局工具很薄；迁移与代码生成命令在应用的依赖图里执行 |
| 工具链 | Rust edition 2024，MSRV 1.94（受 SeaORM 约束） | 本机 1.100 满足 |
| ID 与时间 | scru128 4；chrono 0.4（`Local::now().naive_local()`） | 见风险 R3 |
| 校验 | validator 0.21 | DTO 入参校验 |
| 热替换与文件监听 | arc-swap 1.9、notify 9.0 | 配置、模板、权限缓存；`dev` 监听源码 |
| 日志与指标 | tracing、tracing-subscriber；Silent `metrics` | JSON 日志输出到 stdout |
| OpenAPI | silent-openapi（utoipa 6） | 同时生成前端 TS 类型 |
| 密码 | argon2（Silent `security`） | `make_password`、`verify_password` |
| 令牌 | 不透明令牌 + 服务端存储（默认）；jsonwebtoken 12（可选） | 见 §8 D3 |
| 前端 | Vue 3、Vite、TypeScript、shadcn-vue（Tailwind）、TanStack Query 与 Table、Vue Router、Pinia；yarn | 见 §8 D5 |
| 前端嵌入 | rust-embed 8 | release 内嵌，debug 读磁盘 |
| 代码模板 | tera 2.4 | 与 Silent `template` 同源；允许项目覆盖模板 |
| 部署 | Docker 多阶段 + distroless；Compose；K8s 清单（Helm 可选）；GitHub Actions | 沿用 silent-oss 的镜像实践 |

版本号为 2026-11-25 在 crates.io 检索到的最新版，M0 锁定具体版本。

## 5. 里程碑与优先级

优先级：P0 为没有它就不成立，P1 为首个可用版本必须，P2 为增强。

| 里程碑 | 内容 | 优先级 | 完成标准 |
|---|---|---|---|
| **M0 验证与定稿** | 最小 spike：Silent + SeaORM 2.0 + 迁移 + SQLite 跑通；定稿 §8 决策；建仓库与 CI | P0 | design.md §16 每项验证有结论；D1–D13 定稿；spike 的 `cargo check` 与 `nextest` 通过 |
| **M1 核心底座** | 启动器、脚手架 `new`、分层模板、统一响应与错误、OpenAPI、SeaORM 接入；**迁移核心**：`migrate generate / up / down / status / lint` 与 `generate entity`（包入 sea-orm-cli 的能力，作为应用子命令执行）、框架迁移与应用迁移分表、执行锁、SQLite 事务化；`dev` 运行循环（构建、启动、监听、重启、刷新）和在终端里的分栏面板、`/healthz` 与 `/readyz`、优雅关停 | P0 | `new` 出的项目 5 分钟内可运行；内置示例模块的 CRUD 有 TestClient 集成测试（成功、参数错误、404）；`/swagger` 可访问；SQLite 上从零建库通过，带 `mysql`、`postgres` feature 时能编译，两进程并发执行迁移只跑一次，"旧版本建库后升级"测试通过，`generate entity` 对同样的表输出与基准命令一致；`fmt`、`clippy -D warnings`、`nextest` 全部通过；生成项目 `cargo check` 零警告 |
| **M2 认证与 RBAC** | 登录、刷新、登出；用户、角色、菜单、部门、字典、参数、日志（系统表由框架迁移创建）；认证与权限中间件；管理前端基座（登录、布局、动态菜单） | P0 | 集成测试覆盖错误密码、锁定、过期、撤销、401 与 403；双节点下角色变更 5 秒内生效；前端可登录并按菜单渲染；全新数据库首次启动完成框架迁移并引导默认管理员（随机密码只打印一次） |
| **M3 代码生成（CLI）** | 从表生成 dto、service、api、路由注册、菜单 SQL、前端页面（实体由 `generate entity` 负责）；新表的迁移骨架（`migrate generate --table`）；模板可覆盖 | P0 | 对示例库 10 张表生成后 `cargo check` 通过；重复生成不破坏手改内容；分层规范机检通过；"文章管理"从建表到联调端到端通过 |
| **M4 配置中心与开发体验** | 分层配置、内置 Provider、热更新、变更订阅、管理页；`dev` 增强（浏览器刷新、错误覆盖层、分栏面板已在 M1 之后做掉；剩 OpenAPI 到 TS 类型同步） | P1 | 合并优先级有单测；远端变更 5 秒内生效，非法配置被拒绝并告警；远端不可达时用本地缓存启动；改 Rust 自动重启并刷新浏览器；改配置和模板不重启 |
| **M5 分布式部署与可观测性** | 多副本验证、任务互斥、缓存版本失效、迁移 Job 与滚动发布联动、部署产物与发布流程、监控页 | P1 | 3 副本下令牌互通、定时任务只执行一次（计数验证）、滚动重启 0 个 5xx；先迁移后滚动发布的流程演练通过；K8s 清单在 kind 或 k3s 部署成功；镜像非 root 并记录体积 |
| **M6 在线生成与稳定化** | 管理界面内代码生成、代码热补丁实验、文档与示例、安全检查、发布 | P2 | 在线生成可用；热补丁实验报告；文档覆盖快速开始、指南、API；安全清单通过；发布流程演练 |

依赖关系：M1 → M2 → M3（生成器依赖 M1 的规范、命令行与 M2 的权限声明，并且只处理迁移已执行的库）→ M4 与 M5 可并行 → M6。

## 6. 技术边界与约束

1. **先在仓库内，保持可拆**：以嵌套 workspace 的形式放在 Silent 仓库的 `silent-boot/` 下，用路径依赖引用 `silent`；对外层 workspace、CI 和锁文件没有影响。拆成独立仓库时，把路径依赖改成 crates.io 版本即可。
2. **不在框架里复制 Silent 能力**：中间件、提取器、OpenAPI 直接复用；缺口走上游 PR（见 design.md §15）。
3. **默认安全**：argon2 密码、令牌哈希存储、业务接口未声明权限即拒绝、CORS 白名单、生产环境 Cookie 带 `Secure` 与 `HttpOnly`、登录限流、密钥只从环境变量或 secret 文件读取。
4. **默认无状态**：会话、上传文件、任务状态不落本地盘；SQLite 只定位为单机或开发数据库。
5. **生成即拥有**：生成器输出显式源码，不藏在宏里，不引入运行时反射式 CRUD。
6. **最小 crate 数起步**：先 `silent-boot`（核心）、`silent-boot-cli`（全局工具）、`web/`、`templates/`；配置中心、RBAC 以 feature 提供，编译时间或解耦确有需要时再拆 crate。
7. **生成的代码零警告**：`clippy -D warnings` 是脚手架与生成器的验收线。
8. **迁移优先**：数据库结构只通过迁移变更；实体和业务模块都由迁移执行后的库派生。SeaORM 的迁移器不带执行锁、迁移记录不含校验和，这两点由框架补齐（执行锁、审计、事务化），不 fork。
9. **命令行是唯一入口**：迁移和代码生成都是显式命令，在应用的依赖图里执行以保证版本一致；`dev` 只做运行循环，不自动代劳（面板里的迁移操作是使用者按了键才执行）。sea-orm-cli 保留为后门。
10. **约定即契约**：实体固定生成到应用根目录的 `entity/src/`，索引为 `lib.rs`；`generate entity` 以 `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l` 为基准命令，默认行为与它一致。框架依赖的生成项不提供参数，需要不同布局时直接用 sea-orm-cli。

## 7. 风险与应对

| # | 风险 | 影响 | 应对 |
|---|---|---|---|
| R1 | SeaORM 2.0 发布仅 4 个月，API 与生态仍在变化 | 生成的模板与查询代码需要跟随调整 | 锁定 minor 版本；生成代码只用稳定 API（find、paginate、insert、update、delete）；升级列入例行维护 |
| R2 | MSRV 因 SeaORM 升到 1.94 | 旧工具链无法编译 | 文档明示；CI 用 stable 与 1.94 双工具链 |
| R3 | chrono 被标记为 unmaintained（RUSTSEC-2026-0543） | 后续可能被迫迁移 | 时间类型集中在 `silent_boot::types` 的别名里，业务代码不直接依赖 chrono 的具体类型 |
| R4 | Silent handler 的错误类型固定为 `SilentError` | 业务码只能以 JSON 字符串携带 | 框架封装 `AppError` 到 `SilentError` 的转换；向上游提议可携带任意 payload（design.md §15） |
| R5 | Silent 路由树在启动时冻结 | 无法运行时增删路由，代码热补丁受限 | 代码热补丁定为实验特性；`dev` 采用进程重启 |
| R6 | Silent `session` 基于 async-session，其传递依赖 bincode 已不再维护（RUSTSEC-2025-0141，目前在 `deny.toml` 中忽略） | 依赖不再维护 | 框架自管令牌，不启用 `session` feature |
| R7 | shadcn-vue 是社区移植 | 组件质量与更新节奏不确定 | 组件收敛在 `web/src/components/ui`，便于替换 |
| R8 | 静态资源没有 SPA 回退（`StaticOptions` 只有压缩与目录列表） | 前端 history 路由刷新 404 | 自实现兜底路由，M0 验证与 `<path:**>` 的匹配优先级 |
| R9 | 多节点缓存一致性存在时间窗 | 权限与配置变更延迟生效 | 版本号加短 TTL（目标 5 秒内）；有 Redis 时即时通知；文档说明 |
| R10 | 范围蔓延 | 久拖不决 | 遵守 §3.2；每个里程碑以完成标准收口 |
| R11 | 各数据库的 ALTER 能力不同，MySQL 的 DDL 不能回滚 | 同一条迁移在不同库上行为可能不一致，失败可能留下半成品 | 差异写进设计供写迁移时参考；统一在事务中执行；一条迁移只做一件事；破坏性操作必须标记；`status` 与 `repair` 兜底。只在 SQLite 上自动验证（D14） |
| R12 | SeaORM 迁移器不加锁，迁移记录没有校验和 | 多副本会重复执行；改动历史迁移无法发现 | 框架加执行锁与审计表；`migrate status` 提示已执行迁移被修改 |
| R13 | 包入 sea-orm-cli 的能力后，生成的实体依赖 `sea-orm-codegen` 与 SeaORM 版本一致 | 版本错开时生成的实体可能编不过 | 命令在应用的依赖图里执行；M0 对比两者结果（design.md §16 V14） |
| R14 | 命令行和代码生成的依赖会进生产二进制 | 体积和编译时间增加 | M0 已量化：开启 LTO 和 strip 后，`cli` 约 1.4 MiB，代码生成约 2 MiB。代码生成已拆到默认不开的 `codegen` feature；release profile 要配置 LTO 和 strip |

## 8. 决策记录

2026-11-25 确认：D1 按立项时的情况调整，其余采纳"建议"一列。

| # | 决策 | 建议 | 备选 |
|---|---|---|---|
| D1 | 项目名与仓库 | `silent-boot`；先在 Silent 仓库内以嵌套 workspace 开发（`silent-boot/`），M1 之后再决定是否拆成独立仓库 | 一开始就建独立仓库；名字备选 `silent-stack`、`silent-forge` |
| D2 | RBAC 实现 | 自研轻量 RBAC（用户、角色、菜单权限码），便于多节点失效和生成器集成 | 沿用 Casbin（策略存 DB 加 watcher） |
| D3 | 令牌方案 | 不透明令牌存 DB，可撤销、多副本天然共享，配刷新令牌 | JWT（无状态，但撤销困难） |
| D4 | 配置中心后端优先级 | 内置（DB + 版本号轮询）→ etcd → Nacos | 直接对接 Nacos |
| D5 | 前端栈 | Vue 3 + shadcn-vue + TanStack Query 与 Table；yarn（版本以 `packageManager` 锁定） | Element Plus |
| D6 | 结构变更入口 | 迁移优先：先写迁移并执行，实体由 `generate entity` 生成，模块由生成器读库元数据生成 | 模型定义优先（YAML），由模型生成迁移 |
| D7 | 发布形态 | crates.io 发布 `silent-boot` 与 `silent-boot-cli`；模板随工具内嵌 | 只提供 git 模板 |
| D8 | silent-template 的处置 | 新框架 M1 完成后归档并在 README 指向新框架 | 继续维护 |
| D9 | 迁移编写方式 | Rust DSL 为默认，原生 SQL 按方言分支 | 纯 SQL 文件 |
| D10 | 生产环境迁移执行方式 | 发布流程中独立执行（K8s Job 或 CI 步骤），先迁移后滚动发布 | 应用启动时自动执行（需加锁） |
| D11 | 框架迁移与应用迁移 | 分表管理：`sb_migrations` 与 `seaql_migrations` | 合并到一个迁移列表 |
| D12 | 命令的入口形态 | 应用内子命令（`cargo run -- migrate up`）加全局工具转发；`dev` 默认不执行迁移 | 只做全局工具（需要另行编译迁移 crate）；`dev --migrate` 设为默认 |
| D13 | 审计时间的填充方式 | 由 service 层写入：生成的模板已经写好，实体文件与基准命令一致，和 silent-oss 现在的做法相同 | 由生成器把填充逻辑写进实体末尾的 `ActiveModelBehavior`，自动填充，但覆盖不到批量更新 |
| D14 | 多数据库的验证范围 | 数据库能力全部交给 SeaORM：框架只用 SQLite 做自动化测试；MySQL、PostgreSQL 通过 feature 提供，只保证 `cargo check` 通过，不在 CI 里跑真实数据库（2026-11-26 确认） | 为三种数据库各起一个服务做迁移矩阵 |
| D15 | 待执行迁移的定义（2026-11-26 开发中定下，待你确认） | 按这个版本的迁移列表和跟踪表对账：列表里有、表里没有的才是待执行；表里有、列表里没有的说明数据库比这个版本新，不算问题，旧副本照常就绪、照常启动。一个和线上分叉的版本（两边都有对方没有的迁移）拒绝执行 | 沿用 SeaORM 的对账：见到不认识的已执行迁移就报错（M1 实测到它让滚动发布走不通，见 `m1-verification.md` 的 F16） |

## 9. 任务清单与验证记录

日常任务在 [TODO.md](TODO.md)，按里程碑分节，当前是 M1 收尾与 M2。验证的结论和实测发现记在 [docs/m0-verification.md](docs/m0-verification.md) 和 [docs/m1-verification.md](docs/m1-verification.md)。

环境现状（2026-11-25 本机核对）：Rust 1.100.0、sea-orm-cli 2.0.4、watchexec 2.8.0、cargo generate 0.26.0、Node 22.16.0、yarn 1.22.22、pnpm 9.15.0、Docker 29.8.2；cargo-watch 仓库已归档，不再沿用 silent-template 的 `.cargo-watch.toml` 方案。
