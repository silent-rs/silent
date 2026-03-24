//! 路由文档收集和扩展
//!
//! 提供路由文档自动收集功能和路由扩展trait。

use crate::doc::{
    DocMeta, RequestMeta, ResponseMeta, lookup_doc_by_handler_ptr,
    lookup_extra_responses_by_handler_ptr, lookup_request_by_handler_ptr,
    lookup_response_by_handler_ptr,
};
use crate::{OpenApiDoc, schema::PathInfo};
use silent::prelude::Route;
use utoipa::openapi::{PathItem, ResponseBuilder, path::Operation};

/// 文档化的路由信息
#[derive(Debug, Clone)]
pub struct DocumentedRoute {
    /// Silent框架的原始路由
    pub route: Route,
    /// 路径文档信息
    pub path_docs: Vec<PathInfo>,
}

impl DocumentedRoute {
    /// 创建新的文档化路由
    pub fn new(route: Route) -> Self {
        Self {
            route,
            path_docs: Vec::new(),
        }
    }

    /// 添加路径文档信息
    pub fn add_path_doc(mut self, path_info: PathInfo) -> Self {
        self.path_docs.push(path_info);
        self
    }

    /// 批量添加路径文档
    pub fn add_path_docs(mut self, path_docs: Vec<PathInfo>) -> Self {
        self.path_docs.extend(path_docs);
        self
    }

    /// 获取底层的Silent路由
    pub fn into_route(self) -> Route {
        self.route
    }

    /// 生成OpenAPI路径项
    pub fn generate_path_items(&self, base_path: &str) -> Vec<(String, PathItem)> {
        let mut path_items = Vec::new();

        for path_doc in &self.path_docs {
            let full_path = if base_path.is_empty() {
                path_doc.path.clone()
            } else {
                format!("{}{}", base_path.trim_end_matches('/'), &path_doc.path)
            };

            // 转换Silent路径参数格式到OpenAPI格式
            let openapi_path = convert_path_format(&full_path);

            // 创建操作
            let operation = create_operation_from_path_info(path_doc);

            // 创建或更新路径项
            let path_item = create_or_update_path_item(None, &path_doc.method, operation);

            path_items.push((openapi_path, path_item));
        }

        path_items
    }
}

/// 路由文档收集trait
///
/// 为Silent的Route提供文档收集能力。
pub trait RouteDocumentation {
    /// 收集路由的文档信息
    ///
    /// # 参数
    ///
    /// - `base_path`: 基础路径前缀
    ///
    /// # 返回
    ///
    /// 返回路径和对应的OpenAPI PathItem的映射
    fn collect_openapi_paths(&self, base_path: &str) -> Vec<(String, PathItem)>;

    /// 生成完整的OpenAPI文档
    ///
    /// # 参数
    ///
    /// - `title`: API标题
    /// - `version`: API版本
    /// - `description`: API描述
    ///
    /// # 返回
    ///
    /// 返回完整的OpenAPI文档
    fn generate_openapi_doc(
        &self,
        title: &str,
        version: &str,
        description: Option<&str>,
    ) -> OpenApiDoc {
        let mut doc = OpenApiDoc::new(title, version);

        if let Some(desc) = description {
            doc = doc.description(desc);
        }

        let paths = self.collect_openapi_paths("");
        doc = doc.add_paths(paths).apply_registered_schemas();

        doc
    }
}

impl RouteDocumentation for Route {
    fn collect_openapi_paths(&self, base_path: &str) -> Vec<(String, PathItem)> {
        let mut paths = Vec::new();
        collect_paths_recursive(self, base_path, &[], &mut paths);
        paths
    }
}

/// 从中间件列表推断通用响应码
///
/// 通过中间件类型名称启发式识别常见中间件，自动添加对应的错误响应描述。
fn infer_middleware_responses(
    middlewares: &[std::sync::Arc<dyn silent::MiddleWareHandler>],
) -> Vec<crate::doc::ExtraResponse> {
    let mut responses = Vec::new();
    for mw in middlewares {
        // 使用 std::any::type_name 获取底层类型名称（通过 trait object 的 Any 风格推断）
        let type_name = std::any::type_name_of_val(&**mw).to_string();
        // 限流中间件 → 429
        if type_name.contains("RateLimiter") || type_name.contains("rate_limiter") {
            responses.push(crate::doc::ExtraResponse {
                status: 429,
                description: "Too Many Requests".to_string(),
            });
        }
        // 认证/授权中间件 → 401
        if type_name.contains("Auth") || type_name.contains("auth") {
            responses.push(crate::doc::ExtraResponse {
                status: 401,
                description: "Unauthorized".to_string(),
            });
        }
        // 超时中间件 → 408
        if type_name.contains("Timeout") || type_name.contains("timeout") {
            responses.push(crate::doc::ExtraResponse {
                status: 408,
                description: "Request Timeout".to_string(),
            });
        }
    }
    // 去重
    responses.dedup_by_key(|r| r.status);
    responses
}

/// 递归收集路径信息
///
/// `parent_tags` 用于路由组级别的 tags 继承。
fn collect_paths_recursive(
    route: &Route,
    current_path: &str,
    parent_tags: &[String],
    paths: &mut Vec<(String, PathItem)>,
) {
    let full_path = if current_path.is_empty() {
        route.path.clone()
    } else if route.path.is_empty() {
        current_path.to_string()
    } else {
        format!(
            "{}/{}",
            current_path.trim_end_matches('/'),
            route.path.trim_start_matches('/')
        )
    };

    // 如果当前路由有非空路径段，作为 tag 向下传递
    let mut current_tags = parent_tags.to_vec();
    let seg = route.path.trim_matches('/');
    if !seg.is_empty()
        && !seg.starts_with('<')
        && !seg.starts_with('{')
        && !current_tags.contains(&seg.to_string())
    {
        current_tags.push(seg.to_string());
    }

    // 从中间件推断通用响应码
    let mw_responses = infer_middleware_responses(&route.middlewares);

    // 为当前路径的每个HTTP方法创建操作
    for (method, handler) in &route.handler {
        let openapi_path = convert_path_format(&full_path);
        let ptr = std::sync::Arc::as_ptr(handler) as *const () as usize;
        let doc = lookup_doc_by_handler_ptr(ptr);
        let resp = lookup_response_by_handler_ptr(ptr);
        let req_meta = lookup_request_by_handler_ptr(ptr);
        let mut extra_resp_list = lookup_extra_responses_by_handler_ptr(ptr).unwrap_or_default();

        // 合并中间件推断的响应（避免重复）
        for mw_resp in &mw_responses {
            if !extra_resp_list.iter().any(|r| r.status == mw_resp.status) {
                extra_resp_list.push(mw_resp.clone());
            }
        }
        let extra_resp = if extra_resp_list.is_empty() {
            None
        } else {
            Some(extra_resp_list)
        };

        let operation = create_operation_with_doc(
            method,
            &full_path,
            doc,
            resp,
            req_meta,
            extra_resp,
            &current_tags,
        );
        let path_item = create_or_update_path_item(None, method, operation);

        // 查找是否已存在相同路径
        if let Some((_, existing_item)) = paths.iter_mut().find(|(path, _)| path == &openapi_path) {
            // 更新现有路径项
            *existing_item = merge_path_items(existing_item, &path_item);
        } else {
            paths.push((openapi_path, path_item));
        }
    }

    // 递归处理子路由（传递当前 tags）
    for child in &route.children {
        collect_paths_recursive(child, &full_path, &current_tags, paths);
    }
}

/// 转换Silent路径格式到OpenAPI格式
///
/// Silent: `/users/<id:i64>/posts/<post_id:String>`
/// OpenAPI: `/users/{id}/posts/{post_id}`
fn convert_path_format(silent_path: &str) -> String {
    // 归一化：空路径映射为 "/"；其他路径确保以 '/' 开头，避免 Swagger 生成非法路径键
    let mut result = if silent_path.is_empty() {
        "/".to_string()
    } else if silent_path.starts_with('/') {
        silent_path.to_string()
    } else {
        format!("/{}", silent_path)
    };

    // 查找所有的 <name:type> 模式并替换为 {name}
    while let Some(start) = result.find('<') {
        if let Some(end) = result[start..].find('>') {
            let full_match = &result[start..start + end + 1];
            if let Some(colon_pos) = full_match.find(':') {
                let param_name = &full_match[1..colon_pos];
                let replacement = format!("{{{}}}", param_name);
                result = result.replace(full_match, &replacement);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
}

/// 从PathInfo创建Operation
fn create_operation_from_path_info(path_info: &PathInfo) -> Operation {
    use utoipa::openapi::path::OperationBuilder;

    let mut builder = OperationBuilder::new();

    if let Some(ref operation_id) = path_info.operation_id {
        builder = builder.operation_id(Some(operation_id.clone()));
    }

    if let Some(ref summary) = path_info.summary {
        builder = builder.summary(Some(summary.clone()));
    }

    if let Some(ref description) = path_info.description {
        builder = builder.description(Some(description.clone()));
    }

    if !path_info.tags.is_empty() {
        builder = builder.tags(Some(path_info.tags.clone()));
    }

    // 添加默认响应
    let default_response = ResponseBuilder::new()
        .description("Successful response")
        .build();

    builder = builder.response("200", default_response);

    builder.build()
}

/// 创建默认的Operation
fn create_operation_with_doc(
    method: &http::Method,
    path: &str,
    doc: Option<DocMeta>,
    resp: Option<ResponseMeta>,
    req_meta: Option<Vec<RequestMeta>>,
    extra_resp: Option<Vec<crate::doc::ExtraResponse>>,
    #[allow(unused_variables)] parent_tags: &[String],
) -> Operation {
    use utoipa::openapi::Required;
    use utoipa::openapi::path::{OperationBuilder, ParameterBuilder};

    let default_summary = format!("{} {}", method, path);
    let default_description = format!("Handler for {} {}", method, path);
    let (summary, description, deprecated, custom_tags) = match doc {
        Some(DocMeta {
            summary,
            description,
            deprecated,
            tags,
        }) => (
            summary.unwrap_or(default_summary),
            description.unwrap_or(default_description),
            deprecated,
            tags,
        ),
        None => (default_summary, default_description, false, Vec::new()),
    };

    // 自动生成 operationId（method_去除非字母数字并用下划线连接）
    let sanitized_path: String = path
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c,
            _ => '_',
        })
        .collect();
    let operation_id = format!("{}_{}", method.as_str().to_lowercase(), sanitized_path)
        .trim_matches('_')
        .to_string();

    // 默认 tag：取首个非空路径段
    let default_tag = path
        .split('/')
        .find(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut response_builder = ResponseBuilder::new().description("Successful response");
    if let Some(rm) = resp {
        match rm {
            ResponseMeta::TextPlain => {
                use utoipa::openapi::{
                    RefOr,
                    content::ContentBuilder,
                    schema::{ObjectBuilder, Schema},
                };
                let content = ContentBuilder::new()
                    .schema::<RefOr<Schema>>(Some(RefOr::T(Schema::Object(
                        ObjectBuilder::new().build(),
                    ))))
                    .build();
                response_builder = response_builder.content("text/plain", content);
            }
            ResponseMeta::Json { type_name } => {
                use utoipa::openapi::{Ref, RefOr, content::ContentBuilder, schema::Schema};
                let schema_ref = RefOr::Ref(Ref::from_schema_name(type_name));
                let content = ContentBuilder::new()
                    .schema::<RefOr<Schema>>(Some(schema_ref))
                    .build();
                response_builder = response_builder.content("application/json", content);
            }
        }
    }
    let default_response = response_builder.build();

    // 从路径中提取 Silent 风格参数 <name:type> 或 OpenAPI 风格 {name}，提供基础参数声明
    let mut builder = OperationBuilder::new()
        .summary(Some(summary))
        .description(Some(description))
        .operation_id(Some(operation_id))
        .response("200", default_response);

    // deprecated 标记
    if deprecated {
        builder = builder.deprecated(Some(utoipa::openapi::Deprecated::True));
    }

    // tags 优先级：自定义 tags > 路由组继承 tags > 自动生成 tag
    if !custom_tags.is_empty() {
        builder = builder.tags(Some(custom_tags));
    } else if !parent_tags.is_empty() {
        builder = builder.tags(Some(parent_tags.to_vec()));
    } else if let Some(tag) = default_tag {
        builder = builder.tags(Some(vec![tag]));
    }

    // 额外响应（400、401、404 等）
    if let Some(extras) = extra_resp {
        for er in extras {
            let resp = ResponseBuilder::new().description(er.description).build();
            builder = builder.response(er.status.to_string(), resp);
        }
    }

    // 处理请求元信息：requestBody 和 query parameters
    if let Some(req_metas) = req_meta {
        for meta in req_metas {
            match meta {
                RequestMeta::JsonBody { type_name } => {
                    use utoipa::openapi::{
                        Ref, RefOr, content::ContentBuilder, request_body::RequestBodyBuilder,
                        schema::Schema,
                    };
                    let schema_ref = RefOr::Ref(Ref::from_schema_name(type_name));
                    let content = ContentBuilder::new()
                        .schema::<RefOr<Schema>>(Some(schema_ref))
                        .build();
                    let request_body = RequestBodyBuilder::new()
                        .content("application/json", content)
                        .required(Some(Required::True))
                        .build();
                    builder = builder.request_body(Some(request_body));
                }
                RequestMeta::FormBody { type_name } => {
                    use utoipa::openapi::{
                        Ref, RefOr, content::ContentBuilder, request_body::RequestBodyBuilder,
                        schema::Schema,
                    };
                    let schema_ref = RefOr::Ref(Ref::from_schema_name(type_name));
                    let content = ContentBuilder::new()
                        .schema::<RefOr<Schema>>(Some(schema_ref))
                        .build();
                    let request_body = RequestBodyBuilder::new()
                        .content("application/x-www-form-urlencoded", content)
                        .required(Some(Required::True))
                        .build();
                    builder = builder.request_body(Some(request_body));
                }
                RequestMeta::QueryParams { type_name } => {
                    // 查询参数：添加一个引用 schema 的 query parameter
                    let param = ParameterBuilder::new()
                        .name(type_name)
                        .parameter_in(utoipa::openapi::path::ParameterIn::Query)
                        .required(Required::False)
                        .schema::<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>(Some(
                            utoipa::openapi::RefOr::Ref(utoipa::openapi::Ref::from_schema_name(
                                type_name,
                            )),
                        ))
                        .build();
                    builder = builder.parameter(param);
                }
            }
        }
    }

    // 先尝试解析 Silent 风格 <name:type>
    {
        let mut i = 0usize;
        let mut found_any = false;
        while let Some(start) = path[i..].find('<') {
            let abs_start = i + start;
            if let Some(end_rel) = path[abs_start..].find('>') {
                let abs_end = abs_start + end_rel;
                let inner = &path[abs_start + 1..abs_end];
                let mut it = inner.splitn(2, ':');
                let name = it.next().unwrap_or("");
                let type_hint = it.next().unwrap_or("");

                if !name.is_empty() {
                    let schema = rust_type_to_schema(type_hint);
                    let param = ParameterBuilder::new()
                        .name(name)
                        .parameter_in(utoipa::openapi::path::ParameterIn::Path)
                        .required(Required::True)
                        .schema(schema)
                        .build();
                    builder = builder.parameter(param);
                    found_any = true;
                }
                i = abs_end + 1;
            } else {
                break;
            }
        }

        // 如未找到 Silent 风格参数，则尝试解析 {name}（无类型信息，默认 string）
        if !found_any {
            let mut idx = 0usize;
            while let Some(start) = path[idx..].find('{') {
                let abs_start = idx + start;
                if let Some(end_rel) = path[abs_start..].find('}') {
                    let abs_end = abs_start + end_rel;
                    let name = &path[abs_start + 1..abs_end];
                    let schema = rust_type_to_schema("String");
                    let param = ParameterBuilder::new()
                        .name(name)
                        .parameter_in(utoipa::openapi::path::ParameterIn::Path)
                        .required(Required::True)
                        .schema(schema)
                        .build();
                    builder = builder.parameter(param);
                    idx = abs_end + 1;
                } else {
                    break;
                }
            }
        }
    }

    builder.build()
}

/// 将 Rust 类型名称映射为 OpenAPI Schema
///
/// 支持常见的 Rust 基础类型：整数、浮点、字符串、布尔值。
/// 未识别的类型默认映射为 string。
fn rust_type_to_schema(
    type_hint: &str,
) -> Option<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>> {
    use utoipa::openapi::schema::{ObjectBuilder, Schema, SchemaType, Type};

    let (schema_type, format) = match type_hint {
        // 整数类型
        "i8" | "i16" | "i32" | "u8" | "u16" | "u32" => {
            (SchemaType::Type(Type::Integer), Some("int32"))
        }
        "i64" | "u64" | "i128" | "u128" | "isize" | "usize" => {
            (SchemaType::Type(Type::Integer), Some("int64"))
        }
        // 浮点类型
        "f32" => (SchemaType::Type(Type::Number), Some("float")),
        "f64" => (SchemaType::Type(Type::Number), Some("double")),
        // 布尔
        "bool" => (SchemaType::Type(Type::Boolean), None),
        // 字符串（默认）
        "String" | "str" | "&str" | "" => (SchemaType::Type(Type::String), None),
        // 未知类型也映射为 string
        _ => (SchemaType::Type(Type::String), None),
    };

    let mut builder = ObjectBuilder::new().schema_type(schema_type);
    if let Some(fmt) = format {
        builder = builder.format(Some(utoipa::openapi::schema::SchemaFormat::Custom(
            fmt.to_string(),
        )));
    }
    Some(utoipa::openapi::RefOr::T(Schema::Object(builder.build())))
}

/// 创建或更新PathItem
fn create_or_update_path_item(
    _existing: Option<&PathItem>,
    method: &http::Method,
    operation: Operation,
) -> PathItem {
    let mut item = PathItem::default();
    match *method {
        http::Method::GET => item.get = Some(operation),
        http::Method::POST => item.post = Some(operation),
        http::Method::PUT => item.put = Some(operation),
        http::Method::DELETE => item.delete = Some(operation),
        http::Method::PATCH => item.patch = Some(operation),
        http::Method::HEAD => item.head = Some(operation),
        http::Method::OPTIONS => item.options = Some(operation),
        http::Method::TRACE => item.trace = Some(operation),
        _ => {}
    }
    item
}

/// 合并两个PathItem
fn merge_path_items(item1: &PathItem, item2: &PathItem) -> PathItem {
    let mut out = PathItem::default();
    out.get = item1.get.clone().or(item2.get.clone());
    out.post = item1.post.clone().or(item2.post.clone());
    out.put = item1.put.clone().or(item2.put.clone());
    out.delete = item1.delete.clone().or(item2.delete.clone());
    out.patch = item1.patch.clone().or(item2.patch.clone());
    out.head = item1.head.clone().or(item2.head.clone());
    out.options = item1.options.clone().or(item2.options.clone());
    out.trace = item1.trace.clone().or(item2.trace.clone());
    out
}

/// Route 的便捷 OpenAPI 构建扩展
pub trait RouteOpenApiExt {
    fn to_openapi(&self, title: &str, version: &str) -> utoipa::openapi::OpenApi;
}

impl RouteOpenApiExt for Route {
    fn to_openapi(&self, title: &str, version: &str) -> utoipa::openapi::OpenApi {
        self.generate_openapi_doc(title, version, None)
            .into_openapi()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{DocMeta, RequestMeta, ResponseMeta};

    #[test]
    fn test_path_format_conversion() {
        assert_eq!(
            convert_path_format("/users/<id:i64>/posts"),
            "/users/{id}/posts"
        );

        assert_eq!(
            convert_path_format("/api/v1/users/<user_id:String>/items/<item_id:u32>"),
            "/api/v1/users/{user_id}/items/{item_id}"
        );

        assert_eq!(convert_path_format("/simple/path"), "/simple/path");
        assert_eq!(convert_path_format("svc"), "/svc");
        assert_eq!(convert_path_format(""), "/");
    }

    #[test]
    fn test_documented_route_creation() {
        let route = Route::new("users");
        let doc_route = DocumentedRoute::new(route);

        assert_eq!(doc_route.path_docs.len(), 0);
    }

    #[test]
    fn test_path_info_to_operation() {
        let path_info = PathInfo::new(http::Method::GET, "/users/{id}")
            .operation_id("get_user")
            .summary("获取用户")
            .description("根据ID获取用户信息")
            .tag("users");

        let operation = create_operation_from_path_info(&path_info);

        assert_eq!(operation.operation_id, Some("get_user".to_string()));
        assert_eq!(operation.summary, Some("获取用户".to_string()));
        assert_eq!(
            operation.description,
            Some("根据ID获取用户信息".to_string())
        );
        assert_eq!(operation.tags, Some(vec!["users".to_string()]));
    }

    #[test]
    fn test_documented_route_generate_items() {
        let route = DocumentedRoute::new(Route::new(""))
            .add_path_doc(PathInfo::new(http::Method::GET, "/ping").summary("ping"));
        let items = route.generate_path_items("");
        let (_p, item) = items.into_iter().find(|(p, _)| p == "/ping").unwrap();
        assert!(item.get.is_some());
    }

    #[test]
    fn test_generate_openapi_doc_with_registered_schema() {
        use serde::Serialize;
        use utoipa::ToSchema;
        #[derive(Serialize, ToSchema)]
        struct MyType {
            id: i32,
        }
        crate::doc::register_schema_for::<MyType>();
        let route = Route::new("");
        let openapi = route.generate_openapi_doc("t", "1", None).into_openapi();
        assert!(
            openapi
                .components
                .as_ref()
                .expect("components")
                .schemas
                .contains_key("MyType")
        );
    }

    #[test]
    fn test_collect_paths_with_multiple_methods() {
        async fn h1(_r: silent::Request) -> silent::Result<silent::Response> {
            Ok(silent::Response::text("ok"))
        }
        async fn h2(_r: silent::Request) -> silent::Result<silent::Response> {
            Ok(silent::Response::text("ok"))
        }
        let route = Route::new("svc").get(h1).post(h2);
        let paths = route.collect_openapi_paths("");
        // 找到 /svc 项
        let (_p, item) = paths.into_iter().find(|(p, _)| p == "/svc").expect("/svc");
        assert!(item.get.is_some());
        assert!(item.post.is_some());
    }

    #[test]
    fn test_operation_with_text_plain() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/hello",
            Some(DocMeta {
                summary: Some("s".into()),
                description: Some("d".into()),
                deprecated: false,
                tags: Vec::new(),
            }),
            Some(ResponseMeta::TextPlain),
            None,
            None,
            &[],
        );
        let resp = op.responses.responses.get("200").expect("200 resp");
        let resp = match resp {
            utoipa::openapi::RefOr::T(r) => r,
            _ => panic!("expected T"),
        };
        let content = &resp.content;
        assert!(content.contains_key("text/plain"));
    }

    #[test]
    fn test_operation_with_json_ref() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/users/{id}",
            None,
            Some(ResponseMeta::Json { type_name: "User" }),
            None,
            None,
            &[],
        );
        let resp = op.responses.responses.get("200").expect("200 resp");
        let resp = match resp {
            utoipa::openapi::RefOr::T(r) => r,
            _ => panic!("expected T"),
        };
        let content = &resp.content;
        let mt = content.get("application/json").expect("app/json");
        let schema = mt.schema.as_ref().expect("schema");
        match schema {
            utoipa::openapi::RefOr::Ref(r) => assert!(r.ref_location.ends_with("/User")),
            _ => panic!("ref expected"),
        }
    }

    #[test]
    fn test_operation_with_json_request_body() {
        let op = create_operation_with_doc(
            &http::Method::POST,
            "/users",
            None,
            None,
            Some(vec![RequestMeta::JsonBody {
                type_name: "CreateUser",
            }]),
            None,
            &[],
        );
        let body = op.request_body.as_ref().expect("request body");
        let content = body.content.get("application/json").expect("app/json");
        let schema = content.schema.as_ref().expect("schema");
        match schema {
            utoipa::openapi::RefOr::Ref(r) => {
                assert!(r.ref_location.ends_with("/CreateUser"))
            }
            _ => panic!("ref expected"),
        }
    }

    #[test]
    fn test_operation_with_form_request_body() {
        let op = create_operation_with_doc(
            &http::Method::POST,
            "/login",
            None,
            None,
            Some(vec![RequestMeta::FormBody {
                type_name: "LoginForm",
            }]),
            None,
            &[],
        );
        let body = op.request_body.as_ref().expect("request body");
        assert!(
            body.content
                .contains_key("application/x-www-form-urlencoded")
        );
    }

    #[test]
    fn test_operation_with_query_params() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/search",
            None,
            None,
            Some(vec![RequestMeta::QueryParams {
                type_name: "SearchQuery",
            }]),
            None,
            &[],
        );
        let params = op.parameters.as_ref().expect("should have parameters");
        assert!(!params.is_empty());
    }

    #[test]
    fn test_merge_path_items_get_post() {
        let get = create_or_update_path_item(
            None,
            &http::Method::GET,
            create_operation_with_doc(&http::Method::GET, "/a", None, None, None, None, &[]),
        );
        let post = create_or_update_path_item(
            None,
            &http::Method::POST,
            create_operation_with_doc(&http::Method::POST, "/a", None, None, None, None, &[]),
        );
        let merged = merge_path_items(&get, &post);
        assert!(merged.get.is_some());
        assert!(merged.post.is_some());
    }

    #[test]
    fn test_merge_prefers_first_for_same_method() {
        let op1 = create_operation_with_doc(&http::Method::GET, "/a", None, None, None, None, &[]);
        let mut item1 = PathItem::default();
        item1.get = Some(op1);
        let op2 = create_operation_with_doc(&http::Method::GET, "/a", None, None, None, None, &[]);
        let mut item2 = PathItem::default();
        item2.get = Some(op2);
        let merged = merge_path_items(&item1, &item2);
        assert!(merged.get.is_some());
    }

    #[test]
    fn test_operation_deprecated_flag() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/old",
            Some(DocMeta {
                summary: Some("旧接口".into()),
                description: None,
                deprecated: true,
                tags: Vec::new(),
            }),
            None,
            None,
            None,
            &[],
        );
        assert!(matches!(
            op.deprecated,
            Some(utoipa::openapi::Deprecated::True)
        ));
    }

    #[test]
    fn test_operation_custom_tags() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/users",
            Some(DocMeta {
                summary: None,
                description: None,
                deprecated: false,
                tags: vec!["用户管理".into(), "admin".into()],
            }),
            None,
            None,
            None,
            &[],
        );
        assert_eq!(
            op.tags,
            Some(vec!["用户管理".to_string(), "admin".to_string()])
        );
    }

    #[test]
    fn test_operation_extra_responses() {
        use crate::doc::ExtraResponse;
        let op = create_operation_with_doc(
            &http::Method::POST,
            "/users",
            None,
            None,
            None,
            Some(vec![
                ExtraResponse {
                    status: 400,
                    description: "请求参数无效".into(),
                },
                ExtraResponse {
                    status: 401,
                    description: "未授权".into(),
                },
            ]),
            &[],
        );
        let resp_400 = op.responses.responses.get("400").expect("400 resp");
        let resp_401 = op.responses.responses.get("401").expect("401 resp");
        match resp_400 {
            utoipa::openapi::RefOr::T(r) => assert_eq!(r.description, "请求参数无效"),
            _ => panic!("expected T"),
        }
        match resp_401 {
            utoipa::openapi::RefOr::T(r) => assert_eq!(r.description, "未授权"),
            _ => panic!("expected T"),
        }
    }

    #[test]
    fn test_path_param_type_mapping() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/users/<id:i64>/posts/<slug:String>",
            None,
            None,
            None,
            None,
            &[],
        );
        let params = op.parameters.as_ref().expect("should have parameters");
        assert_eq!(params.len(), 2);
        // 验证第一个参数有 schema（不再是 None）
        let id_param = &params[0];
        assert!(id_param.schema.is_some());
        let slug_param = &params[1];
        assert!(slug_param.schema.is_some());
    }

    #[test]
    fn test_parent_tags_inheritance() {
        let op = create_operation_with_doc(
            &http::Method::GET,
            "/users/123",
            None,
            None,
            None,
            None,
            &["users".to_string()],
        );
        // 没有自定义 tags，应继承 parent_tags
        assert_eq!(op.tags, Some(vec!["users".to_string()]));
    }

    #[test]
    fn test_rust_type_to_schema_integer() {
        let schema = rust_type_to_schema("i64");
        assert!(schema.is_some());
    }

    #[test]
    fn test_rust_type_to_schema_boolean() {
        let schema = rust_type_to_schema("bool");
        assert!(schema.is_some());
    }

    #[test]
    fn test_rust_type_to_schema_default_string() {
        let schema = rust_type_to_schema("");
        assert!(schema.is_some());
    }
}
