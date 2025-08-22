//! 路由文档收集和扩展
//!
//! 提供路由文档自动收集功能和路由扩展trait。

use crate::{schema::PathInfo, OpenApiDoc};
use silent::prelude::Route;
use utoipa::openapi::{path::Operation, PathItem, ResponseBuilder};

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
        doc = doc.add_paths(paths);

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
    for (method, _handler) in &route.handler {
        let openapi_path = convert_path_format(&full_path);
        let operation = create_default_operation(method, &full_path);
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
    // 简化实现：使用字符串替换而不是regex
    let mut result = silent_path.to_string();

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
fn create_default_operation(method: &http::Method, path: &str) -> Operation {
    use utoipa::openapi::path::{OperationBuilder, ParameterBuilder};
    use utoipa::openapi::Required;

    let summary = format!("{} {}", method, path);
    let description = format!("Handler for {} {}", method, path);

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

    let default_response = ResponseBuilder::new()
        .description("Successful response")
        .build();

    // 从路径中提取形如 {id} 的 path params，提供基础参数声明（string，后续可增强类型推断）
    let mut builder = OperationBuilder::new()
        .summary(Some(summary))
        .description(Some(description))
        .operation_id(Some(operation_id))
        .response("200", default_response);

    if let Some(tag) = default_tag {
        builder = builder.tags(Some(vec![tag]));
    }

    let mut idx = 0usize;
    while let Some(start) = path[idx..].find('{') {
        let abs_start = idx + start;
        if let Some(end_rel) = path[abs_start..].find('}') {
            let abs_end = abs_start + end_rel;
            let name = &path[abs_start + 1..abs_end];
            // 简化为 string 类型的必填 path 参数
            let param = ParameterBuilder::new()
                .name(name)
                .parameter_in(utoipa::openapi::path::ParameterIn::Path)
                .required(Required::True)
                .build();
            builder = builder.parameter(param);
            idx = abs_end + 1;
        } else {
            break;
        }
    }

    builder.build()
}

/// 创建或更新PathItem
fn create_or_update_path_item(
    existing: Option<&PathItem>,
    method: &http::Method,
    operation: Operation,
) -> PathItem {
    use utoipa::openapi::path::PathItemBuilder;

    let mut builder = if let Some(_existing) = existing {
        // 从现有PathItem创建builder（这里简化处理）
        PathItemBuilder::new()
    } else {
        PathItemBuilder::new()
    };

    // 根据HTTP方法设置操作（简化实现）
    match method {
        &http::Method::GET => {
            builder = builder.operation(utoipa::openapi::PathItemType::Get, operation)
        }
        &http::Method::POST => {
            builder = builder.operation(utoipa::openapi::PathItemType::Post, operation)
        }
        &http::Method::PUT => {
            builder = builder.operation(utoipa::openapi::PathItemType::Put, operation)
        }
        &http::Method::DELETE => {
            builder = builder.operation(utoipa::openapi::PathItemType::Delete, operation)
        }
        &http::Method::PATCH => {
            builder = builder.operation(utoipa::openapi::PathItemType::Patch, operation)
        }
        &http::Method::HEAD => {
            builder = builder.operation(utoipa::openapi::PathItemType::Head, operation)
        }
        &http::Method::OPTIONS => {
            builder = builder.operation(utoipa::openapi::PathItemType::Options, operation)
        }
        &http::Method::TRACE => {
            builder = builder.operation(utoipa::openapi::PathItemType::Trace, operation)
        }
        _ => {} // 其他方法暂不支持
    }

    builder.build()
}

/// 合并两个PathItem
fn merge_path_items(item1: &PathItem, item2: &PathItem) -> PathItem {
    use std::collections::BTreeSet;
    use utoipa::openapi::path::PathItemBuilder;

    let mut builder = PathItemBuilder::new();
    let mut existing: BTreeSet<utoipa::openapi::PathItemType> = BTreeSet::new();

    for (ty, op) in item1.operations.iter() {
        builder = builder.operation(ty.clone(), op.clone());
        existing.insert(ty.clone());
    }

    for (ty, op) in item2.operations.iter() {
        if !existing.contains(ty) {
            builder = builder.operation(ty.clone(), op.clone());
        }
    }

    builder.build()
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
