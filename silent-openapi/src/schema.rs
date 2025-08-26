//! OpenAPI 文档模式和工具
//!
//! 提供创建和管理OpenAPI文档的工具函数和类型定义。

use crate::{OpenApiError, Result};
use serde_json::Value;
use utoipa::openapi::{InfoBuilder, OpenApi, OpenApiBuilder, PathItem, PathsBuilder};

/// OpenAPI文档构建器
///
/// 提供便捷的方法来构建和管理OpenAPI文档。
#[derive(Clone)]
pub struct OpenApiDoc {
    /// 内部的OpenAPI对象
    openapi: OpenApi,
}

impl OpenApiDoc {
    /// 创建一个新的OpenAPI文档
    ///
    /// # 参数
    ///
    /// - `title`: API标题
    /// - `version`: API版本
    ///
    /// # 示例
    ///
    /// ```rust
    /// use silent_openapi::OpenApiDoc;
    ///
    /// let doc = OpenApiDoc::new("用户API", "1.0.0");
    /// ```
    pub fn new(title: &str, version: &str) -> Self {
        let openapi = OpenApiBuilder::new()
            .info(InfoBuilder::new().title(title).version(version).build())
            .build();

        Self { openapi }
    }

    /// 由现有的 OpenApi 对象创建文档包装
    pub fn from_openapi(openapi: OpenApi) -> Self {
        Self { openapi }
    }

    /// 设置API描述
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        if let Some(ref mut info) = self.openapi.info.description {
            *info = description.into();
        } else {
            self.openapi.info.description = Some(description.into());
        }
        self
    }

    /// 添加服务器信息
    ///
    /// # 参数
    ///
    /// - `url`: 服务器URL
    /// - `description`: 服务器描述
    pub fn add_server(mut self, url: &str, description: Option<&str>) -> Self {
        use utoipa::openapi::ServerBuilder;

        let mut server_builder = ServerBuilder::new().url(url);
        if let Some(desc) = description {
            server_builder = server_builder.description(Some(desc));
        }

        let server = server_builder.build();

        if self.openapi.servers.is_none() {
            self.openapi.servers = Some(Vec::new());
        }

        if let Some(ref mut servers) = self.openapi.servers {
            servers.push(server);
        }

        self
    }

    /// 添加路径项
    ///
    /// # 参数
    ///
    /// - `path`: API路径
    /// - `path_item`: 路径项定义
    pub fn add_path(mut self, path: &str, path_item: PathItem) -> Self {
        // 简化实现：创建一个新的paths对象
        let mut paths_builder = PathsBuilder::new();
        paths_builder = paths_builder.path(path, path_item);
        self.openapi.paths = paths_builder.build();
        self
    }

    /// 批量添加路径
    ///
    /// # 参数
    ///
    /// - `paths`: 路径映射表
    pub fn add_paths(mut self, paths: Vec<(String, PathItem)>) -> Self {
        let mut paths_builder = PathsBuilder::new();
        for (path, path_item) in paths {
            paths_builder = paths_builder.path(&path, path_item);
        }
        self.openapi.paths = paths_builder.build();
        self
    }

    /// 为给定类型名追加占位 schema（占位 Object，用于引用解析）
    pub fn add_placeholder_schemas(mut self, type_names: &[&str]) -> Self {
        use utoipa::openapi::ComponentsBuilder;
        use utoipa::openapi::schema::{ObjectBuilder, Schema};
        let mut components = self
            .openapi
            .components
            .unwrap_or_else(|| ComponentsBuilder::new().build());
        for name in type_names {
            components
                .schemas
                .entry((*name).to_string())
                .or_insert_with(|| {
                    utoipa::openapi::RefOr::T(Schema::Object(ObjectBuilder::new().build()))
                });
        }
        self.openapi.components = Some(components);
        self
    }

    /// 添加 Bearer/JWT 安全定义
    pub fn add_bearer_auth(mut self, scheme_name: &str, description: Option<&str>) -> Self {
        use utoipa::openapi::ComponentsBuilder;
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

        let http = HttpBuilder::new()
            .scheme(HttpAuthScheme::Bearer)
            .bearer_format("JWT");
        if let Some(_desc) = description {
            // 某些版本的 utoipa 暂不支持在 HttpBuilder 直接设置 description，这里跳过
        }
        let scheme = SecurityScheme::Http(http.build());

        let mut components = self
            .openapi
            .components
            .unwrap_or_else(|| ComponentsBuilder::new().build());
        components
            .security_schemes
            .insert(scheme_name.to_string(), scheme);
        self.openapi.components = Some(components);
        self
    }

    /// 设置全局 security 要求
    pub fn set_global_security(mut self, scheme_name: &str, scopes: &[&str]) -> Self {
        use utoipa::openapi::security::SecurityRequirement;

        let scopes_vec: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        let requirement = SecurityRequirement::new(scheme_name.to_string(), scopes_vec);

        match self.openapi.security {
            Some(ref mut list) => list.push(requirement),
            None => self.openapi.security = Some(vec![requirement]),
        }
        self
    }

    /// 获取内部的OpenAPI对象
    pub fn openapi(&self) -> &OpenApi {
        &self.openapi
    }

    /// 转换为OpenAPI对象
    pub fn into_openapi(self) -> OpenApi {
        self.openapi
    }

    /// 序列化为JSON字符串
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.openapi).map_err(OpenApiError::Json)
    }

    /// 序列化为格式化的JSON字符串
    pub fn to_pretty_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.openapi).map_err(OpenApiError::Json)
    }

    /// 序列化为JSON Value
    pub fn to_json_value(&self) -> Result<Value> {
        serde_json::to_value(&self.openapi).map_err(OpenApiError::Json)
    }
}

/// 路径信息
///
/// 用于描述API路径的基本信息。
#[derive(Debug, Clone)]
pub struct PathInfo {
    /// HTTP方法
    pub method: http::Method,
    /// 路径模式
    pub path: String,
    /// 操作ID
    pub operation_id: Option<String>,
    /// 摘要
    pub summary: Option<String>,
    /// 描述
    pub description: Option<String>,
    /// 标签
    pub tags: Vec<String>,
}

impl PathInfo {
    /// 创建新的路径信息
    pub fn new(method: http::Method, path: &str) -> Self {
        Self {
            method,
            path: path.to_string(),
            operation_id: None,
            summary: None,
            description: None,
            tags: Vec::new(),
        }
    }

    /// 设置操作ID
    pub fn operation_id<S: Into<String>>(mut self, id: S) -> Self {
        self.operation_id = Some(id.into());
        self
    }

    /// 设置摘要
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// 设置描述
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 添加标签
    pub fn tag<S: Into<String>>(mut self, tag: S) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 设置多个标签
    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(|s| s.into()).collect();
        self
    }
}

/// 创建基础的成功响应
pub fn create_success_response(description: &str) -> utoipa::openapi::Response {
    use utoipa::openapi::ResponseBuilder;

    ResponseBuilder::new().description(description).build()
}

/// 创建JSON响应
pub fn create_json_response(
    description: &str,
    _schema_ref: Option<&str>,
) -> utoipa::openapi::Response {
    use utoipa::openapi::ResponseBuilder;

    // 简化实现，暂时不处理schema引用
    ResponseBuilder::new().description(description).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_doc_creation() {
        let doc = OpenApiDoc::new("Test API", "1.0.0")
            .description("A test API")
            .add_server("http://localhost:8080", Some("Development server"));

        let openapi = doc.openapi();
        assert_eq!(openapi.info.title, "Test API");
        assert_eq!(openapi.info.version, "1.0.0");
        assert_eq!(openapi.info.description, Some("A test API".to_string()));
        assert!(openapi.servers.is_some());
    }

    #[test]
    fn test_path_info() {
        let path_info = PathInfo::new(http::Method::GET, "/users/{id}")
            .operation_id("get_user")
            .summary("Get user by ID")
            .description("Retrieve a user by their unique identifier")
            .tag("users");

        assert_eq!(path_info.method, http::Method::GET);
        assert_eq!(path_info.path, "/users/{id}");
        assert_eq!(path_info.operation_id, Some("get_user".to_string()));
        assert_eq!(path_info.tags, vec!["users"]);
    }

    #[test]
    fn test_json_serialization() {
        let doc = OpenApiDoc::new("Test API", "1.0.0");
        let json = doc.to_json().unwrap();
        assert!(json.contains("Test API"));
        assert!(json.contains("1.0.0"));
    }
}
