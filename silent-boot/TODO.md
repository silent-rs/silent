# TODO

## M0 验证与定稿（已完成）

验证项全部有结论，实测发现 F1–F15 在 `docs/m0-verification.md`。

## M1 核心底座（本地完成，待 CI）

完成标准见 `PLAN.md` §5，逐条对照和实测发现 F16–F25 在 `docs/m1-verification.md`。

分支：`feature/fullstack-framework`，从 `main`（d7fadde）迁出。8 个本地提交，**没有推送**。

### 已完成

- [x] 启动器 `App` / `AppBuilder`：读配置、连库、装配中间件和系统路由、挂业务路由、优雅关停；`silent_boot::run` 是应用的唯一入口
- [x] 约定配置 `config/app.toml`（加环境专属文件）和 `APP__` 前缀的环境变量；取值校验，不认识的键报错
- [x] `/healthz`、`/readyz`（有待执行的迁移或数据库连不上才未就绪）
- [x] 统一响应与错误：`Envelope`、`AppError`、`AppResult`；`TraceId`、`ErrorHandler` 两个中间件，提取器的拒绝响应和 panic 都进包络（F1）
- [x] SeaORM 接入：`DbRegistry` 的命名连接、连接池参数、SQLite 地址自动建目录；分页 `Page`、`PageQuery`、`PageResult`、`paginate`
- [x] 带校验的提取器 `ValidatedJson`、`ValidatedQuery`
- [x] 前端静态资源与 SPA 兜底；`base_path`
- [x] OpenAPI：`openapi` feature，挂载 Swagger UI，`Envelope`、`PageResult` 可以直接写进文档
- [x] `dev` 和启动器打通：`--migrate`、待执行迁移的提示、页面脚本自动注入
- [x] 脚手架 `new`：迁移 crate、实体 crate、分层的示例模块、接口测试、配置、Makefile、README、AGENTS.md；示例应用 `examples/demo`
- [x] `migrate lint`、`baseline`、`repair`
- [x] `healthcheck` 子命令
- [x] `mysql`、`postgres` feature：能编译
- [x] 迁移对账容得下更新的数据库；执行锁显式释放；"旧版本建库后升级"和"两个进程同时迁移"的测试
- [x] `generate module` 的第一步：按表生成 dto、service、api 并登记
- [x] MSRV 1.94 下整个嵌套 workspace 和示例应用都能 `cargo check`

### 待办

- [ ] 推送分支、开草稿 PR，让 CI（含示例应用的作业）在 GitHub 上实际跑一遍——需要你确认后再推送

## 用户还没定的事

- [ ] `deny.toml` 是否忽略 RUSTSEC-2026-0543（chrono unmaintained）。现在提交时用 `SKIP=cargo-deny` 跳过这一个钩子，其余钩子照常
- [ ] 是否推送并开 PR
- [ ] M1 之后是否把 silent-boot 拆成独立仓库（PLAN.md 的 D1）

## M1 留到后面的小项

- [ ] `migrate down / fresh / refresh / reset`（只在开发环境可用）
- [ ] 所有子命令的 `--json` 输出（现在只有 `migrate status`）
- [ ] 命名连接的迁移目录和 `--db <name>`
- [ ] Silent 的 `Logger` 中间件的 `peer` 只读 `x-real-ip`，没有就是 `-`；框架自己的访问日志里补上 TCP 对端和可信代理解析（配合 M2 的 `trusted_proxies`）
- [ ] `AppConfig` 里加 `auth`、`rbac`、`config_center` 段（随 M2、M4）

## M2 认证与 RBAC（未开始）

完成标准见 `PLAN.md` §5。按依赖顺序：

1. [ ] 系统表的框架迁移：`sys_user`、`sys_role`、`sys_menu`、`sys_dept`、`sys_dict`、`sys_config`、`sys_log`（设计 §7.1）
2. [ ] 不透明令牌：签发、校验、刷新、登出、撤销，令牌哈希存储（D3）
3. [ ] 登录接口：argon2 校验、失败次数锁定、限流；首次启动引导默认管理员（随机密码只打印一次）
4. [ ] `CurrentUser` 提取器、认证中间件、`user.require("模块:资源:动作")` 的权限校验
5. [ ] `trusted_proxies` 配置和真实 IP 解析
6. [ ] 配置里的 `auth`、`rbac` 段
7. [ ] 双节点下角色变更 5 秒内生效（版本号加短 TTL）
8. [ ] 管理前端基座：Vue 3 + Vite + shadcn-vue，登录页、布局、动态菜单；Vite 开发服务器代理 `/api`（D5）
9. [ ] 集成测试：错误密码、锁定、过期、撤销、401 与 403；多副本令牌互通
