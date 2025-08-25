# Swagger P0 开发需求整理（不含UI静态资源本地化）

## 背景
- 目标：在现有 `silent-openapi` 基础上，完善核心 OpenAPI 生成与整合能力，便于对接生产环境 API 文档。
- 范围：本次不处理 UI 静态资源本地化，聚焦于合并逻辑、安全定义与易用性 API。

## 范围与目标
- PathItem 合并：合并同一路径下多 HTTP 方法定义，避免覆盖/丢失。
- 安全定义：提供 Bearer/JWT `securitySchemes` 与全局 `security` 支持。
- 易用性 API：
  - `Route` → `OpenApi` 便捷方法（如 `Route::to_openapi(title, version)`）。
  - 默认 `operationId`/`tags`/`summary` 生成，并从路径 `{param}` 推断基础 path 参数。
 - UI 配置：提供开关以禁用 Try it out（不涉及静态资源本地化）。
 - Swagger UI 挂载：`SwaggerUiHandler::into_route()` 与 `RouterAdapt`，可直接 `Route::append(handler)`。

## 详细需求
1) PathItem 合并
   - 合并规则：保留已存在方法，新增缺失方法；冲突优先保留先前定义。
   - 不改变 summary/description 等顶层字段（后续规划再处理）。

2) 安全定义
   - `OpenApiDoc::add_bearer_auth(name, description)`：添加 `http` `bearer` (`JWT`) 方案。
   - `OpenApiDoc::set_global_security(name, scopes)`：设置全局 `security`。
   - 示例：新增 `examples/security_example.rs`，展示受保护路由与文档配置。

3) 易用性 API 与默认生成
   - `RouteOpenApiExt::to_openapi(title, version)`：基于路由自动生成基础文档。
   - 默认 tags：取首个非空路径段，如 `/users/{id}` → `users`。
   - 默认 operationId：`<method>_<sanitized_path>`，如 `get__users__{id}`。
   - path 参数：从 `{param}` 生成必填的 path parameter（基础版本不推断类型，后续增强）。

4) UI 行为开关
   - `SwaggerUiMiddleware::with_options` / `SwaggerUiHandler::with_options`：支持 `tryItOutEnabled` 开关。
   - 默认开启；生产可由业务禁用（本次不做自动环境判断）。

## 验收标准
- `cargo check -p silent-openapi --examples` 通过。
- 同一路径下 `GET/POST/...` 方法在 OpenAPI 文档中同时存在且不覆盖。
- OpenAPI JSON 中存在 `components.securitySchemes.bearerAuth` 与全局 `security`。
- 示例运行后 `/docs` UI 可正常访问，`Try it out` 可根据选项开关。

## 变更影响
- 对外 API：新增若干便捷方法与 UI 选项，保持现有接口向后兼容。
- 风险：与 utoipa 版本 API 差异相关（已按 4.2.x 适配）。
