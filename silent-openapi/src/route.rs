//! 路由文档收集和扩展
//!
//! 提供路由文档自动收集功能和路由扩展trait。

use crate::doc::{
    DocMeta, ResponseMeta, list_registered_json_types, lookup_doc_by_handler_ptr,
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
        collect_paths_recursive(self, base_path, &mut paths);
        paths
    }
}

/// 递归收集路径信息
fn collect_paths_recursive(route: &Route, current_path: &str, paths: &mut Vec<(String, PathItem)>) {
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

    // 为当前路径的每个HTTP方法创建操作
    for (method, handler) in &route.handler {
        let openapi_path = convert_path_format(&full_path);
        let ptr = std::sync::Arc::as_ptr(handler) as *const () as usize;
        let doc = lookup_doc_by_handler_ptr(ptr);
        let resp = lookup_response_by_handler_ptr(ptr);
        let operation = create_operation_with_doc(method, &full_path, doc, resp);
        let path_item = create_or_update_path_item(None, method, operation);

        // 查找是否已存在相同路径
        if let Some((_, existing_item)) = paths.iter_mut().find(|(path, _)| path == &openapi_path) {
            // 更新现有路径项
            *existing_item = merge_path_items(existing_item, &path_item);
        } else {
            paths.push((openapi_path, path_item));
        }
    }

    // 递归处理子路由
    for child in &route.children {
        collect_paths_recursive(child, &full_path, paths);
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
) -> Operation {
    use utoipa::openapi::Required;
    use utoipa::openapi::path::{OperationBuilder, ParameterBuilder};

    let default_summary = format!("{} {}", method, path);
    let default_description = format!("Handler for {} {}", method, path);
    let (summary, description) = match doc {
        Some(DocMeta {
            summary,
            description,
        }) => (
            summary.unwrap_or(default_summary),
            description.unwrap_or(default_description),
        ),
        None => (default_summary, default_description),
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
        .filter(|s| !s.is_empty())
        .next()
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

    if let Some(tag) = default_tag {
        builder = builder.tags(Some(vec![tag]));
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

                if !name.is_empty() {
                    let param = ParameterBuilder::new()
                        .name(name)
                        .parameter_in(utoipa::openapi::path::ParameterIn::Path)
                        .required(Required::True)
                        .schema::<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>(None)
                        .build();
                    builder = builder.parameter(param);
                    found_any = true;
                }
                i = abs_end + 1;
            } else {
                break;
            }
        }

        // 如未找到 Silent 风格参数，则尝试解析 {name}
        if !found_any {
            let mut idx = 0usize;
            while let Some(start) = path[idx..].find('{') {
                let abs_start = idx + start;
                if let Some(end_rel) = path[abs_start..].find('}') {
                    let abs_end = abs_start + end_rel;
                    let name = &path[abs_start + 1..abs_end];
                    let param = ParameterBuilder::new()
                        .name(name)
                        .parameter_in(utoipa::openapi::path::ParameterIn::Path)
                        .required(Required::True)
                        .schema::<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>(None)
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
}
