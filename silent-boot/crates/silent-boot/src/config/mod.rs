//! 约定配置：`config/app.toml` 加 `APP__` 前缀的环境变量，见 `docs/design.md` §3。
//!
//! 加载顺序（后面的覆盖前面的）：
//!
//! 1. 内置默认值；
//! 2. `config/app.toml`；
//! 3. `config/app.<env>.toml`，环境取自环境变量 `APP_ENV`，没设时取 `app.toml` 里的 `app.env`，再没有就是 `development`；
//! 4. 环境变量：前缀 `APP__`，双下划线分层，例如 `APP__DATABASE__URL`、`APP__DATABASE__MIGRATE__AUTO_APPLY`。
//!
//! 环境变量的值按 TOML 字面量解析：`true` 是布尔，`16` 是整数，`["a","b"]` 是数组，其余按字符串。
//! 不认识的键会报错，拼错的配置不会悄悄失效。

pub mod units;

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
pub use units::{ByteSize, Duration};

pub use units::ParseUnitError;

/// 配置目录，相对于应用根目录。
pub const CONFIG_DIR: &str = "config";
/// 环境变量前缀。
pub const ENV_PREFIX: &str = "APP__";
/// 指定运行环境的环境变量。
pub const ENV_VAR: &str = "APP_ENV";

/// 运行环境。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Env {
    /// 本机开发：默认环境，Swagger 文档默认开启，日志默认是人读的格式。
    #[default]
    Development,
    /// 自动化测试。
    Test,
    /// 线上：Swagger 文档默认关闭，日志默认输出 JSON。
    Production,
}

impl Env {
    /// 配置文件和日志里用的名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Production => "production",
        }
    }

    /// 是否是线上环境。
    pub fn is_production(self) -> bool {
        self == Self::Production
    }
}

impl fmt::Display for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 应用启动时是否顺带执行迁移，见 `docs/design.md` §6.7。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoApply {
    /// 不执行（默认）：结构只由 `migrate up` 改变。
    #[default]
    Never,
    /// 启动时持锁执行一次，适合单实例的小工具。
    Always,
}

/// 日志格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// 人读的多行格式。
    #[default]
    Pretty,
    /// 每行一个 JSON 对象，交给日志采集器。
    Json,
}

/// 完整的应用配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    /// `[app]`
    pub app: AppSection,
    /// `[log]`
    pub log: LogConfig,
    /// `[server]`
    pub server: ServerConfig,
    /// `[database]`
    pub database: DatabaseConfig,
    /// `[cors]`
    pub cors: CorsConfig,
    /// `[openapi]`
    pub openapi: OpenApiConfig,
}

/// `[app]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppSection {
    /// 应用名，写进日志和 OpenAPI 文档的标题。
    pub name: String,
    /// 运行环境。由 `APP_ENV` 决定，配置文件里也可以写。
    pub env: Env,
    /// 部署在子路径时填写，例如 `"/admin"`；根路径留空。
    pub base_path: String,
}

impl Default for AppSection {
    fn default() -> Self {
        Self {
            name: "app".to_owned(),
            env: Env::Development,
            base_path: String::new(),
        }
    }
}

/// `[log]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    /// 日志级别或过滤规则，例如 `"info"`、`"debug,sea_orm=warn"`。
    pub level: String,
    /// 输出格式。不写时：线上是 JSON，其余是人读的格式。
    pub format: Option<LogFormat>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: None,
        }
    }
}

/// `[server]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// 监听地址。默认只监听本机，容器里改成 `0.0.0.0:8080`。
    pub listen: String,
    /// 收到关停信号后等待在途请求的时间。
    pub graceful_shutdown: Duration,
    /// 单个请求的处理时限。
    pub request_timeout: Duration,
    /// 请求体上限。
    pub max_body_size: ByteSize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8080".to_owned(),
            graceful_shutdown: Duration::from_secs(30),
            request_timeout: Duration::from_secs(30),
            max_body_size: ByteSize::mib(10),
        }
    }
}

/// `[database]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    /// 数据库地址：`sqlite://…`、`mysql://…`、`postgres://…`。
    pub url: String,
    /// 连接池上限。
    pub max_connections: u32,
    /// 连接池保持的最少连接数。
    pub min_connections: u32,
    /// 建立连接的时限。
    pub connect_timeout: Duration,
    /// `[database.migrate]`
    pub migrate: MigrateConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://data/app.db".to_owned(),
            max_connections: 16,
            min_connections: 0,
            connect_timeout: Duration::from_secs(8),
            migrate: MigrateConfig::default(),
        }
    }
}

/// `[database.migrate]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MigrateConfig {
    /// 应用启动时是否执行迁移。
    pub auto_apply: AutoApply,
    /// 等待执行锁的时间。
    pub lock_timeout: Duration,
}

impl Default for MigrateConfig {
    fn default() -> Self {
        Self {
            auto_apply: AutoApply::Never,
            lock_timeout: Duration::from_secs(30),
        }
    }
}

/// `[cors]`。`allowed_origins` 为空时不装配 CORS。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CorsConfig {
    /// 允许的来源，例如 `["http://localhost:5173"]`；`["*"]` 表示任意来源。
    pub allowed_origins: Vec<String>,
    /// 是否允许带凭据（Cookie）。与 `*` 不能同时使用。
    pub allow_credentials: bool,
}

/// `[openapi]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenApiConfig {
    /// 是否提供 `/swagger`。不写时：非线上环境开启。
    pub enabled: Option<bool>,
    /// Swagger UI 的挂载路径。
    pub path: String,
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            path: "/swagger".to_owned(),
        }
    }
}

impl OpenApiConfig {
    /// 当前环境下是否提供文档。
    pub fn is_enabled(&self, env: Env) -> bool {
        self.enabled.unwrap_or(!env.is_production())
    }
}

impl AppConfig {
    /// 从当前目录加载配置，环境变量取自进程环境。
    pub fn load() -> Result<Self, ConfigError> {
        let vars = std::env::vars_os()
            .filter_map(|(key, value)| {
                let (key, value) = (key.into_string().ok()?, value.into_string().ok()?);
                Some((key, value))
            })
            .collect::<Vec<_>>();
        let root = std::env::current_dir().map_err(|source| ConfigError::Read {
            path: PathBuf::from("."),
            source,
        })?;
        Self::load_from(&root, vars)
    }

    /// 从 `root/config` 加载配置，环境变量由 `vars` 给出（名称在前，值在后）。
    pub fn load_from<I>(root: &Path, vars: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let vars: Vec<(String, String)> = vars.into_iter().collect();
        let dir = root.join(CONFIG_DIR);

        let mut value = read_optional(&dir.join("app.toml"))?.unwrap_or_else(empty_table);
        let env = match find(&vars, ENV_VAR) {
            Some(raw) => Some(Env::parse(raw).map_err(|value| ConfigError::InvalidEnv {
                name: ENV_VAR.to_owned(),
                value,
            })?),
            None => value
                .get("app")
                .and_then(|app| app.get("env"))
                .and_then(|env| env.as_str())
                .and_then(|env| Env::parse(env).ok()),
        };
        let env = env.unwrap_or_default();
        merge(
            &mut value,
            read_optional(&dir.join(format!("app.{env}.toml")))?,
        );
        apply_env(&mut value, &vars)?;
        let mut config: AppConfig = value
            .try_into()
            .map_err(|source| ConfigError::Parse { source })?;
        // 环境以 APP_ENV 为准，其次是 app.toml 里的 app.env；环境专属文件改不了它。
        config.app.env = env;
        config.validate()?;
        Ok(config)
    }

    /// 检查取值是否可用。
    pub fn validate(&self) -> Result<(), ConfigError> {
        let name = &self.app.name;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ConfigError::Invalid(
                "app.name 不能为空，只能包含字母、数字、下划线和连字符".to_owned(),
            ));
        }
        check_base_path(&self.app.base_path)?;

        self.server.listen.parse::<SocketAddr>().map_err(|_| {
            ConfigError::Invalid(format!(
                "server.listen 不是有效的地址：{:?}，应写成 \"127.0.0.1:8080\"",
                self.server.listen
            ))
        })?;
        check_non_zero("server.graceful_shutdown", self.server.graceful_shutdown)?;
        check_non_zero("server.request_timeout", self.server.request_timeout)?;
        if self.server.max_body_size.0 == 0 {
            return Err(ConfigError::Invalid(
                "server.max_body_size 必须大于 0".to_owned(),
            ));
        }

        let url = &self.database.url;
        if url.is_empty() {
            return Err(ConfigError::Invalid(
                "database.url 不能为空，例如 \"sqlite://data/app.db\"".to_owned(),
            ));
        }
        check_database_scheme(url)?;
        if self.database.max_connections == 0 {
            return Err(ConfigError::Invalid(
                "database.max_connections 必须大于 0".to_owned(),
            ));
        }
        check_non_zero("database.connect_timeout", self.database.connect_timeout)?;
        check_non_zero(
            "database.migrate.lock_timeout",
            self.database.migrate.lock_timeout,
        )?;

        for origin in &self.cors.allowed_origins {
            if origin != "*" && !(origin.starts_with("http://") || origin.starts_with("https://")) {
                return Err(ConfigError::Invalid(format!(
                    "cors.allowed_origins 里的 {origin:?} 不是有效的来源，应写成 \"https://example.com\" 或 \"*\""
                )));
            }
        }
        if self.cors.allow_credentials && self.cors.allowed_origins.iter().any(|o| o == "*") {
            return Err(ConfigError::Invalid(
                "cors.allow_credentials 不能和 \"*\" 同时使用，请列出具体的来源".to_owned(),
            ));
        }

        check_base_path(&self.openapi.path)?;
        if self.openapi.path.is_empty() {
            return Err(ConfigError::Invalid(
                "openapi.path 不能为空，例如 \"/swagger\"".to_owned(),
            ));
        }
        Ok(())
    }

    /// 当前环境下的日志设置：过滤规则和格式。
    pub fn log_settings(&self) -> (String, LogFormat) {
        let format = self.log.format.unwrap_or(match self.app.env {
            Env::Production => LogFormat::Json,
            _ => LogFormat::Pretty,
        });
        (self.log.level.clone(), format)
    }
}

impl Env {
    fn parse(text: &str) -> Result<Self, InvalidEnv> {
        match text.trim() {
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "production" => Ok(Self::Production),
            _ => Err(InvalidEnv(text.to_owned())),
        }
    }
}

fn find<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    vars.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn check_base_path(path: &str) -> Result<(), ConfigError> {
    let valid = path.is_empty()
        || (path.starts_with('/') && (path == "/" || !path.ends_with('/')) && !path.contains("//"));
    if valid {
        return Ok(());
    }
    Err(ConfigError::Invalid(format!(
        "路径 {path:?} 必须以 / 开头、不以 / 结尾，例如 \"/admin\""
    )))
}

fn check_non_zero(name: &str, value: Duration) -> Result<(), ConfigError> {
    if value.0.is_zero() {
        return Err(ConfigError::Invalid(format!("{name} 必须大于 0")));
    }
    Ok(())
}

/// 数据库地址的协议要和编译时启用的 feature 对得上。
fn check_database_scheme(url: &str) -> Result<(), ConfigError> {
    let known = [
        ("sqlite:", cfg!(feature = "sqlite"), "sqlite"),
        ("mysql:", cfg!(feature = "mysql"), "mysql"),
        ("postgres:", cfg!(feature = "postgres"), "postgres"),
    ];
    for (scheme, enabled, feature) in known {
        if url.starts_with(scheme) {
            if enabled {
                return Ok(());
            }
            return Err(ConfigError::Invalid(format!(
                "database.url 用的是 {scheme}，但当前构建没有启用 {feature} feature"
            )));
        }
    }
    Err(ConfigError::Invalid(format!(
        "不认识的数据库地址 {url:?}，支持 sqlite://、mysql:// 和 postgres://"
    )))
}

fn read_optional(path: &Path) -> Result<Option<toml::Value>, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let value: toml::Value = toml::from_str(&text).map_err(|source| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(value))
}

/// 把 `from` 并进 `into`：表逐层合并，数组和标量整体替换。
fn merge(into: &mut toml::Value, from: Option<toml::Value>) {
    let Some(from) = from else {
        return;
    };
    match (into, from) {
        (toml::Value::Table(into), toml::Value::Table(from)) => {
            for (key, value) in from {
                match into.get_mut(&key) {
                    Some(slot) => merge(slot, Some(value)),
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
        (into, from) => *into = from,
    }
}

/// 一张空表。
fn empty_table() -> toml::Value {
    toml::Value::Table(Default::default())
}

/// 环境变量的值按 TOML 字面量解析：能写成 `v = <值>` 的按 TOML 的类型来（布尔、整数、数组……），
/// 其余按字符串。地址、密码这类值直接原样使用。
fn parse_env_value(raw: &str) -> toml::Value {
    let parsed = toml::from_str::<toml::Value>(&format!("v = {raw}"))
        .ok()
        .and_then(|value| value.get("v").cloned());
    parsed.unwrap_or_else(|| toml::Value::String(raw.to_owned()))
}

/// 把 `APP__` 前缀的环境变量写进配置树。
fn apply_env(value: &mut toml::Value, vars: &[(String, String)]) -> Result<(), ConfigError> {
    let mut sorted: Vec<&(String, String)> = vars
        .iter()
        .filter(|(k, _)| k.starts_with(ENV_PREFIX))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, raw) in sorted {
        let path: Vec<String> = name[ENV_PREFIX.len()..]
            .split("__")
            .map(|segment| segment.to_ascii_lowercase())
            .collect();
        if path.iter().any(String::is_empty) {
            return Err(ConfigError::Invalid(format!(
                "环境变量 {name} 的层数不完整，应写成 APP__DATABASE__URL 这样"
            )));
        }
        insert(value, &path, parse_env_value(raw), name)?;
    }
    Ok(())
}

fn insert(
    root: &mut toml::Value,
    path: &[String],
    value: toml::Value,
    name: &str,
) -> Result<(), ConfigError> {
    let mut current = root;
    for segment in &path[..path.len() - 1] {
        if !current.is_table() {
            return Err(ConfigError::Invalid(format!(
                "环境变量 {name} 对应的配置路径 {:?} 的中间一段不是表",
                path.join(".")
            )));
        }
        let table = current.as_table_mut().expect("checked above");
        current = table.entry(segment).or_insert_with(empty_table);
    }
    if !current.is_table() {
        return Err(ConfigError::Invalid(format!(
            "环境变量 {name} 对应的配置路径 {:?} 的中间一段不是表",
            path.join(".")
        )));
    }
    current
        .as_table_mut()
        .expect("checked above")
        .insert(path[path.len() - 1].clone(), value);
    Ok(())
}

/// 配置加载失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// 读不了配置文件。
    #[error("读取配置文件 {path} 失败：{source}")]
    Read {
        /// 读不了的文件。
        path: PathBuf,
        /// 文件系统的错误。
        source: std::io::Error,
    },
    /// `config/app.toml` 不是合法的 TOML。
    #[error("解析 {path} 失败：{source}")]
    ParseToml {
        /// 出错的文件。
        path: PathBuf,
        /// TOML 解析器给出的错误。
        source: toml::de::Error,
    },
    /// 合并之后的配置和约定的结构对不上：字段名拼错、类型不对，或者有这里不认识的键。
    #[error("配置的取值不对：{source}")]
    Parse {
        /// 反序列化给出的错误，里面有出问题的字段名。
        source: toml::de::Error,
    },
    /// `APP_ENV` 的取值不对。
    #[error("{name} 的取值 {value:?} 不认识，可选 development、test、production")]
    InvalidEnv {
        /// 环境变量的名字。
        name: String,
        /// 不认识的取值。
        value: InvalidEnv,
    },
    /// 取值通过了结构检查，但内容不可用。
    #[error("{0}")]
    Invalid(String),
}

/// `APP_ENV` 的取值不对。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidEnv(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> AppConfig {
        let value: toml::Value = toml::from_str(text).unwrap();
        value.try_into().unwrap()
    }

    fn temp_root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(CONFIG_DIR)).unwrap();
        (dir, root)
    }

    #[test]
    fn defaults_are_valid() {
        AppConfig::default().validate().unwrap();
        let (level, format) = AppConfig::default().log_settings();
        assert_eq!(level, "info");
        assert_eq!(format, LogFormat::Pretty);
    }

    #[test]
    fn base_file_is_overlaid_by_env_file_and_env_vars() {
        let (dir, root) = temp_root();
        std::fs::write(
            root.join(CONFIG_DIR).join("app.toml"),
            r#"
[app]
name = "demo"

[server]
listen = "0.0.0.0:9000"
request_timeout = "45s"

[database]
url = "sqlite://data/base.db"
max_connections = 16
"#,
        )
        .unwrap();
        std::fs::write(
            root.join(CONFIG_DIR).join("app.test.toml"),
            r#"
[app]
name = "demo-test"

[database]
max_connections = 4
"#,
        )
        .unwrap();

        let config =
            AppConfig::load_from(&root, [(ENV_VAR.to_owned(), "test".to_owned())]).unwrap();
        assert_eq!(config.app.name, "demo-test");
        assert_eq!(config.server.listen, "0.0.0.0:9000");
        assert_eq!(config.server.request_timeout, Duration::from_secs(45));
        assert_eq!(config.database.url, "sqlite://data/base.db");
        assert_eq!(config.database.max_connections, 4);
        assert_eq!(config.app.env, Env::Test);

        let config = AppConfig::load_from(&root, Vec::<(String, String)>::new()).unwrap();
        // 没设 APP_ENV：按 development 加载，不读 app.test.toml。
        assert_eq!(config.app.name, "demo");
        assert_eq!(config.app.env, Env::Development);
        assert_eq!(config.database.max_connections, 16);

        let config = AppConfig::load_from(
            &root,
            [
                (ENV_VAR.to_owned(), "test".to_owned()),
                (
                    "APP__DATABASE__URL".to_owned(),
                    "sqlite::memory:".to_owned(),
                ),
                ("APP__DATABASE__MAX_CONNECTIONS".to_owned(), "2".to_owned()),
                (
                    "APP__DATABASE__MIGRATE__AUTO_APPLY".to_owned(),
                    "always".to_owned(),
                ),
                ("APP__OPENAPI__ENABLED".to_owned(), "false".to_owned()),
            ],
        )
        .unwrap();
        assert_eq!(config.database.url, "sqlite::memory:");
        assert_eq!(config.database.max_connections, 2);
        assert_eq!(config.database.migrate.auto_apply, AutoApply::Always);
        assert_eq!(config.openapi.enabled, Some(false));
        assert!(!config.openapi.is_enabled(Env::Test));

        let _ = dir;
    }

    #[test]
    fn env_var_takes_precedence_over_file_env() {
        let (dir, root) = temp_root();
        std::fs::write(
            root.join(CONFIG_DIR).join("app.toml"),
            "[app]\nname = \"a\"\nenv = \"production\"\n",
        )
        .unwrap();
        let config = AppConfig::load_from(&root, Vec::<(String, String)>::new()).unwrap();
        assert_eq!(config.app.env, Env::Production);
        assert_eq!(config.log_settings().1, LogFormat::Json);

        let config =
            AppConfig::load_from(&root, [(ENV_VAR.to_owned(), "development".to_owned())]).unwrap();
        assert_eq!(config.app.env, Env::Development);
        let _ = dir;
    }

    #[test]
    fn without_any_file_the_defaults_apply() {
        let (dir, root) = temp_root();
        let config = AppConfig::load_from(&root, Vec::<(String, String)>::new()).unwrap();
        assert_eq!(config, AppConfig::default());
        let _ = dir;
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let (dir, root) = temp_root();
        std::fs::write(
            root.join(CONFIG_DIR).join("app.toml"),
            "[server]\nlisten = \"127.0.0.1:8080\"\ngraceful_shutdow = \"1s\"\n",
        )
        .unwrap();
        let err = AppConfig::load_from(&root, Vec::<(String, String)>::new()).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
        assert!(err.to_string().contains("graceful_shutdow"), "{err}");

        let err = AppConfig::load_from(
            &root,
            [("APP__DATBASE__URL".to_owned(), "sqlite://x.db".to_owned())],
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
        let _ = dir;
    }

    #[test]
    fn values_are_checked() {
        let err = parse("[app]\nbase_path = \"admin\"\n")
            .validate()
            .unwrap_err();
        assert!(
            err.to_string().contains("base_path") || err.to_string().contains("admin"),
            "{err}"
        );

        let err = parse("[server]\nlisten = \"8080\"\n")
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("server.listen"), "{err}");

        let err = parse("[database]\nurl = \"\"\n").validate().unwrap_err();
        assert!(err.to_string().contains("database.url"), "{err}");

        let err = parse("[database]\nurl = \"oracle://h/db\"\n")
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("oracle"), "{err}");

        // 协议认识，但编译时没有启用对应的 feature。
        if !cfg!(feature = "mysql") {
            let err = parse("[database]\nurl = \"mysql://u@h/db\"\n")
                .validate()
                .unwrap_err();
            assert!(err.to_string().contains("mysql"), "{err}");
        }
        if !cfg!(feature = "postgres") {
            let err = parse("[database]\nurl = \"postgres://u@h/db\"\n")
                .validate()
                .unwrap_err();
            assert!(err.to_string().contains("postgres"), "{err}");
        }

        let err = parse("[cors]\nallowed_origins = [\"*\"]\nallow_credentials = true\n")
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("allow_credentials"), "{err}");

        let err = parse("[cors]\nallowed_origins = [\"localhost:5173\"]\n")
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("allowed_origins"), "{err}");

        parse("[cors]\nallowed_origins = [\"https://a.example\", \"*\"]\n")
            .validate()
            .unwrap();
        parse("[openapi]\npath = \"/docs\"\n").validate().unwrap();
    }

    #[test]
    fn duration_and_size_accept_units() {
        let config = parse(
            "[server]\nlisten = \"127.0.0.1:8080\"\ngraceful_shutdown = \"1m 30s\"\nmax_body_size = \"2MiB\"\n",
        );
        assert_eq!(config.server.graceful_shutdown, Duration::from_secs(90));
        assert_eq!(config.server.max_body_size, ByteSize::mib(2));
    }

    #[test]
    fn env_values_parse_as_toml_literals() {
        let (dir, root) = temp_root();
        let config = AppConfig::load_from(
            &root,
            [
                (
                    "APP__CORS__ALLOWED_ORIGINS".to_owned(),
                    "[\"https://a.example\", \"https://b.example\"]".to_owned(),
                ),
                ("APP__CORS__ALLOW_CREDENTIALS".to_owned(), "true".to_owned()),
                ("APP__SERVER__LISTEN".to_owned(), "127.0.0.1:0".to_owned()),
            ],
        )
        .unwrap();
        assert_eq!(
            config.cors.allowed_origins,
            vec![
                "https://a.example".to_owned(),
                "https://b.example".to_owned()
            ]
        );
        assert!(config.cors.allow_credentials);
        assert_eq!(config.server.listen, "127.0.0.1:0");
        let _ = dir;
    }

    #[test]
    fn invalid_env_value_is_reported() {
        let (dir, root) = temp_root();
        let err =
            AppConfig::load_from(&root, [(ENV_VAR.to_owned(), "staging".to_owned())]).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidEnv { .. }), "{err}");

        let err = AppConfig::load_from(
            &root,
            [(
                "APP__SERVER__GRACEFUL_SHUTDOWN".to_owned(),
                "soon".to_owned(),
            )],
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
        let _ = dir;
    }
}
