use crate::core::next::Next;
use crate::extractor::FromRequest;
use crate::middleware::MiddleWareHandler;
use crate::{Handler, Request, Response, Result, SilentError, StatusCode};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[cfg(feature = "security")]
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

/// JWT认证配置
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// JWT签名密钥
    pub secret: String,
    /// 算法类型，默认HS256
    pub algorithm: Algorithm,
    /// 可选的受众验证
    pub audience: Option<String>,
    /// 可选的发行者验证
    pub issuer: Option<String>,
    /// 是否验证过期时间
    pub validate_exp: bool,
    /// 是否验证签发时间
    pub validate_nbf: bool,
    /// 允许的时钟偏差（秒）
    pub leeway: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            algorithm: Algorithm::HS256,
            audience: None,
            issuer: None,
            validate_exp: true,
            validate_nbf: true,
            leeway: 60, // 1分钟时钟偏差
        }
    }
}

impl JwtConfig {
    /// 创建新的JWT配置
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            ..Default::default()
        }
    }

    /// 设置算法
    pub fn algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// 设置受众
    pub fn audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// 设置发行者
    pub fn issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// 设置是否验证过期时间
    pub fn validate_exp(mut self, validate: bool) -> Self {
        self.validate_exp = validate;
        self
    }

    /// 设置时钟偏差
    pub fn leeway(mut self, leeway: u64) -> Self {
        self.leeway = leeway;
        self
    }
}

/// 标准JWT声明
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// 主题（用户ID）
    pub sub: String,
    /// 签发时间
    pub iat: u64,
    /// 过期时间
    pub exp: u64,
    /// 生效时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<u64>,
    /// 受众
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// 发行者
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// JWT ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// 自定义声明
    #[serde(flatten)]
    pub custom: serde_json::Value,
}

impl Claims {
    /// 创建新的声明
    pub fn new(subject: impl Into<String>, expires_in: u64) -> Self {
        let now = chrono::Utc::now().timestamp() as u64;
        Self {
            sub: subject.into(),
            iat: now,
            exp: now + expires_in,
            nbf: None,
            aud: None,
            iss: None,
            jti: None,
            custom: serde_json::Value::Null,
        }
    }

    /// 设置自定义声明
    pub fn with_custom(mut self, custom: serde_json::Value) -> Self {
        self.custom = custom;
        self
    }

    /// 设置受众
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.aud = Some(audience.into());
        self
    }

    /// 设置发行者
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.iss = Some(issuer.into());
        self
    }

    /// 设置JWT ID
    pub fn with_jti(mut self, jti: impl Into<String>) -> Self {
        self.jti = Some(jti.into());
        self
    }
}

/// 自定义JWT验证函数类型
pub type JwtValidator = Arc<dyn Fn(&Claims) -> bool + Send + Sync>;

/// JWT认证中间件
pub struct JwtAuth {
    config: JwtConfig,
    /// 跳过认证的路径
    skip_paths: HashSet<String>,
    /// 可选的自定义验证函数
    custom_validator: Option<JwtValidator>,
}

impl Clone for JwtAuth {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            skip_paths: self.skip_paths.clone(),
            custom_validator: self.custom_validator.clone(),
        }
    }
}

impl JwtAuth {
    /// 创建新的JWT认证中间件
    pub fn new(config: JwtConfig) -> Self {
        Self {
            config,
            skip_paths: HashSet::new(),
            custom_validator: None,
        }
    }

    /// 添加跳过认证的路径
    pub fn skip_path(mut self, path: impl Into<String>) -> Self {
        self.skip_paths.insert(path.into());
        self
    }

    /// 添加多个跳过认证的路径
    pub fn skip_paths<I, S>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.skip_paths.extend(paths.into_iter().map(|p| p.into()));
        self
    }

    /// 设置自定义验证函数
    pub fn with_validator<F>(mut self, validator: F) -> Self
    where
        F: Fn(&Claims) -> bool + Send + Sync + 'static,
    {
        self.custom_validator = Some(Arc::new(validator));
        self
    }

    /// 从请求中提取JWT token
    fn extract_token(&self, req: &Request) -> Option<String> {
        // 优先从Authorization header中提取
        if let Some(auth_header) = req.headers().get("authorization")
            && let Ok(auth_str) = auth_header.to_str()
            && let Some(token) = auth_str.strip_prefix("Bearer ")
        {
            return Some(token.to_string());
        }

        // 从query参数中提取token
        if let Some(query) = req.uri().query() {
            for pair in query.split('&') {
                if let Some((key, value)) = pair.split_once('=')
                    && key == "token"
                {
                    return Some(value.to_string());
                }
            }
        }

        None
    }

    /// 验证JWT token
    #[cfg(feature = "security")]
    fn validate_token(&self, token: &str) -> Result<Claims> {
        let decoding_key = DecodingKey::from_secret(self.config.secret.as_ref());

        let mut validation = Validation::new(self.config.algorithm);
        validation.validate_exp = self.config.validate_exp;
        validation.validate_nbf = self.config.validate_nbf;
        validation.leeway = self.config.leeway;

        if let Some(ref aud) = self.config.audience {
            validation.set_audience(&[aud]);
        }

        if let Some(ref iss) = self.config.issuer {
            validation.set_issuer(&[iss]);
        }

        let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
            SilentError::business_error(StatusCode::UNAUTHORIZED, format!("JWT验证失败: {}", e))
        })?;

        // 执行自定义验证
        if let Some(ref validator) = self.custom_validator
            && !validator(&token_data.claims)
        {
            return Err(SilentError::business_error(
                StatusCode::UNAUTHORIZED,
                "自定义JWT验证失败",
            ));
        }

        Ok(token_data.claims)
    }

    #[cfg(not(feature = "security"))]
    fn validate_token(&self, _token: &str) -> Result<Claims> {
        Err(SilentError::business_error(
            StatusCode::NOT_IMPLEMENTED,
            "JWT功能需要启用security特性",
        ))
    }
}

#[async_trait]
impl MiddleWareHandler for JwtAuth {
    async fn match_req(&self, req: &Request) -> bool {
        // 检查是否在跳过路径列表中
        let path = req.uri().path();
        !self.skip_paths.contains(path)
    }

    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        // 提取token
        let token = self.extract_token(&req).ok_or_else(|| {
            SilentError::business_error(StatusCode::UNAUTHORIZED, "缺少JWT token")
        })?;

        // 验证token
        let claims = self.validate_token(&token)?;

        // 将claims存储到请求扩展中，供后续处理器使用
        req.extensions_mut().insert(claims);

        // 继续处理请求
        next.call(req).await
    }
}

/// JWT工具函数
pub struct JwtUtils;

impl JwtUtils {
    /// 生成JWT token
    #[cfg(feature = "security")]
    pub fn encode(claims: &Claims, secret: &str, algorithm: Algorithm) -> Result<String> {
        let encoding_key = EncodingKey::from_secret(secret.as_ref());
        let header = Header::new(algorithm);

        encode(&header, claims, &encoding_key).map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("JWT编码失败: {}", e),
            )
        })
    }

    #[cfg(not(feature = "security"))]
    pub fn encode(_claims: &Claims, _secret: &str, _algorithm: Algorithm) -> Result<String> {
        Err(SilentError::business_error(
            StatusCode::NOT_IMPLEMENTED,
            "JWT功能需要启用security特性",
        ))
    }

    /// 解码JWT token（不验证签名）
    #[cfg(feature = "security")]
    pub fn decode_without_validation(token: &str) -> Result<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.insecure_disable_signature_validation();
        validation.validate_exp = false;
        validation.validate_nbf = false;

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret("".as_ref()),
            &validation,
        )
        .map_err(|e| {
            SilentError::business_error(StatusCode::BAD_REQUEST, format!("JWT解码失败: {}", e))
        })?;

        Ok(token_data.claims)
    }

    #[cfg(not(feature = "security"))]
    pub fn decode_without_validation(_token: &str) -> Result<Claims> {
        Err(SilentError::business_error(
            StatusCode::NOT_IMPLEMENTED,
            "JWT功能需要启用security特性",
        ))
    }
}

/// 便捷的JWT构建器
pub struct JwtBuilder {
    config: JwtConfig,
    skip_paths: Vec<String>,
}

impl JwtBuilder {
    /// 创建新的JWT构建器
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            config: JwtConfig::new(secret),
            skip_paths: Vec::new(),
        }
    }

    /// 设置算法
    pub fn algorithm(mut self, algorithm: Algorithm) -> Self {
        self.config = self.config.algorithm(algorithm);
        self
    }

    /// 设置受众
    pub fn audience(mut self, audience: impl Into<String>) -> Self {
        self.config = self.config.audience(audience);
        self
    }

    /// 设置发行者
    pub fn issuer(mut self, issuer: impl Into<String>) -> Self {
        self.config = self.config.issuer(issuer);
        self
    }

    /// 添加跳过路径
    pub fn skip_path(mut self, path: impl Into<String>) -> Self {
        self.skip_paths.push(path.into());
        self
    }

    /// 构建JWT认证中间件
    pub fn build(self) -> JwtAuth {
        let mut jwt_auth = JwtAuth::new(self.config);
        for path in self.skip_paths {
            jwt_auth = jwt_auth.skip_path(path);
        }
        jwt_auth
    }
}

/// JWT声明萃取器
///
/// 用于从请求中提取JWT认证后的用户声明信息
/// 必须与JwtAuth中间件配合使用
///
/// # 示例
///
/// ```rust
/// use silent::{Route, silent, Result};
/// use silent::middleware::middlewares::{JwtAuth, JwtConfig, Jwt};
///
/// async fn protected_handler(jwt: Jwt) -> Result<String> {
///     Ok(format!("Hello, user: {}", jwt.sub))
/// }
///
/// #[silent::main]
/// async fn main() -> Result<()> {
///     let jwt_config = JwtConfig::new("my-secret-key");
///     let jwt_middleware = JwtAuth::new(jwt_config);
///
///     let route = Route::new("/api")
///         .push(Route::new("/protected").get(protected_handler))
///         .hook(jwt_middleware);
///
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Jwt {
    /// JWT声明
    #[cfg(feature = "security")]
    pub claims: Claims,
    #[cfg(not(feature = "security"))]
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(feature = "security")]
impl Jwt {
    /// 获取用户ID（subject）
    pub fn user_id(&self) -> &str {
        &self.claims.sub
    }

    /// 获取签发时间
    pub fn issued_at(&self) -> u64 {
        self.claims.iat
    }

    /// 获取过期时间
    pub fn expires_at(&self) -> u64 {
        self.claims.exp
    }

    /// 检查是否已过期
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp() as u64;
        self.claims.exp < now
    }

    /// 获取受众
    pub fn audience(&self) -> Option<&str> {
        self.claims.aud.as_deref()
    }

    /// 获取发行者
    pub fn issuer(&self) -> Option<&str> {
        self.claims.iss.as_deref()
    }

    /// 获取JWT ID
    pub fn jwt_id(&self) -> Option<&str> {
        self.claims.jti.as_deref()
    }

    /// 获取自定义声明
    pub fn custom_claims(&self) -> &serde_json::Value {
        &self.claims.custom
    }

    /// 获取自定义声明中的特定字段
    pub fn get_claim<T>(&self, key: &str) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.claims
            .custom
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 检查用户是否具有特定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.get_claim::<Vec<String>>("roles")
            .map(|roles| roles.contains(&role.to_string()))
            .unwrap_or(false)
    }

    /// 检查用户是否具有特定权限
    pub fn has_permission(&self, permission: &str) -> bool {
        self.get_claim::<Vec<String>>("permissions")
            .map(|perms| perms.contains(&permission.to_string()))
            .unwrap_or(false)
    }

    /// 获取用户角色列表
    pub fn roles(&self) -> Vec<String> {
        self.get_claim::<Vec<String>>("roles").unwrap_or_default()
    }

    /// 获取用户权限列表
    pub fn permissions(&self) -> Vec<String> {
        self.get_claim::<Vec<String>>("permissions")
            .unwrap_or_default()
    }
}

#[cfg(not(feature = "security"))]
impl Jwt {
    /// 在未启用security特性时，所有方法都返回错误
    pub fn user_id(&self) -> &str {
        panic!("JWT功能需要启用security特性")
    }
}

impl std::ops::Deref for Jwt {
    #[cfg(feature = "security")]
    type Target = Claims;
    #[cfg(not(feature = "security"))]
    type Target = std::marker::PhantomData<()>;

    fn deref(&self) -> &Self::Target {
        #[cfg(feature = "security")]
        return &self.claims;
        #[cfg(not(feature = "security"))]
        return &self._phantom;
    }
}

#[async_trait]
impl FromRequest for Jwt {
    type Rejection = SilentError;

    #[cfg(feature = "security")]
    async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        // 从请求扩展中获取JWT声明
        // 这些声明应该由JwtAuth中间件预先验证并存储
        let claims = req
            .extensions()
            .get::<Claims>()
            .ok_or_else(|| {
                SilentError::business_error(
                    StatusCode::UNAUTHORIZED,
                    "缺少JWT认证信息，请确保已使用JwtAuth中间件",
                )
            })?
            .clone();

        Ok(Jwt { claims })
    }

    #[cfg(not(feature = "security"))]
    async fn from_request(_req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        Err(SilentError::business_error(
            StatusCode::NOT_IMPLEMENTED,
            "JWT功能需要启用security特性",
        ))
    }
}

/// 可选的JWT萃取器
///
/// 当JWT认证是可选的时候使用，不会在缺少JWT时返回错误
///
/// # 示例
///
/// ```rust
/// use silent::{Route, silent, Result};
/// use silent::middleware::middlewares::OptionalJwt;
///
/// async fn optional_auth_handler(jwt: OptionalJwt) -> Result<String> {
///     match jwt.0 {
///         Some(jwt_claims) => Ok(format!("Hello, authenticated user: {}", jwt_claims.user_id())),
///         None => Ok("Hello, anonymous user".to_string()),
///     }
/// }
/// ```
#[derive(Debug)]
pub struct OptionalJwt(pub Option<Jwt>);

impl OptionalJwt {
    /// 检查是否已认证
    pub fn is_authenticated(&self) -> bool {
        self.0.is_some()
    }

    /// 获取JWT声明（如果存在）
    pub fn claims(&self) -> Option<&Jwt> {
        self.0.as_ref()
    }

    /// 获取用户ID（如果已认证）
    pub fn user_id(&self) -> Option<&str> {
        self.0.as_ref().map(|jwt| jwt.user_id())
    }
}

#[async_trait]
impl FromRequest for OptionalJwt {
    type Rejection = SilentError;

    #[cfg(feature = "security")]
    async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        // 尝试获取JWT声明，如果不存在则返回None
        let claims = req.extensions().get::<Claims>().cloned();

        let jwt = claims.map(|claims| Jwt { claims });
        Ok(OptionalJwt(jwt))
    }

    #[cfg(not(feature = "security"))]
    async fn from_request(_req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
        Ok(OptionalJwt(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;

    #[test]
    fn test_claims_creation() {
        let claims = Claims::new("user123", 3600)
            .with_audience("my-app")
            .with_issuer("auth-service");

        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.aud, Some("my-app".to_string()));
        assert_eq!(claims.iss, Some("auth-service".to_string()));
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_jwt_config() {
        let config = JwtConfig::new("secret")
            .algorithm(Algorithm::HS512)
            .audience("test-app")
            .issuer("test-issuer")
            .leeway(120);

        assert_eq!(config.secret, "secret");
        assert_eq!(config.algorithm, Algorithm::HS512);
        assert_eq!(config.audience, Some("test-app".to_string()));
        assert_eq!(config.leeway, 120);
    }

    #[test]
    fn test_jwt_builder() {
        let jwt_auth = JwtBuilder::new("secret")
            .algorithm(Algorithm::HS256)
            .audience("my-app")
            .skip_path("/health")
            .skip_path("/metrics")
            .build();

        assert_eq!(jwt_auth.config.secret, "secret");
        assert_eq!(jwt_auth.config.algorithm, Algorithm::HS256);
        assert!(jwt_auth.skip_paths.contains("/health"));
        assert!(jwt_auth.skip_paths.contains("/metrics"));
    }

    #[tokio::test]
    async fn test_extract_token_from_header() {
        let mut req = Request::empty();
        req.headers_mut().insert(
            "authorization",
            "Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9"
                .parse()
                .unwrap(),
        );

        let jwt_auth = JwtAuth::new(JwtConfig::new("secret"));
        let token = jwt_auth.extract_token(&req);

        assert_eq!(
            token,
            Some("eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9".to_string())
        );
    }

    #[cfg(feature = "security")]
    mod security_tests {
        use super::*;
        use serde_json::json;

        #[test]
        fn test_jwt_convenience_methods() {
            let custom_claims = json!({
                "roles": ["admin", "user"],
                "permissions": ["read", "write"],
                "department": "engineering"
            });

            let claims = Claims::new("user123", 3600).with_custom(custom_claims);

            let jwt = Jwt { claims };

            assert_eq!(jwt.user_id(), "user123");
            assert!(!jwt.is_expired());
            assert!(jwt.has_role("admin"));
            assert!(jwt.has_role("user"));
            assert!(!jwt.has_role("guest"));
            assert!(jwt.has_permission("read"));
            assert!(jwt.has_permission("write"));
            assert!(!jwt.has_permission("delete"));

            let department: Option<String> = jwt.get_claim("department");
            assert_eq!(department, Some("engineering".to_string()));

            let roles = jwt.roles();
            assert_eq!(roles, vec!["admin", "user"]);

            let permissions = jwt.permissions();
            assert_eq!(permissions, vec!["read", "write"]);
        }

        #[test]
        fn test_optional_jwt() {
            let claims = Claims::new("user123", 3600);
            let jwt = Some(Jwt { claims });
            let optional_jwt = OptionalJwt(jwt);

            assert!(optional_jwt.is_authenticated());
            assert_eq!(optional_jwt.user_id(), Some("user123"));

            let empty_jwt = OptionalJwt(None);
            assert!(!empty_jwt.is_authenticated());
            assert_eq!(empty_jwt.user_id(), None);
        }
    }
}
