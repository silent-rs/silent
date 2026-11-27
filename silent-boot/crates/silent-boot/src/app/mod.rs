//! 启动器：读配置、连库、装配中间件和系统路由、挂业务路由、优雅关停。见 `docs/design.md` §3。
//!
//! ```no_run
//! use silent_boot::prelude::*;
//!
//! # async fn start() -> Result<(), silent_boot::app::BootError> {
//! let app = App::with_config(AppConfig::load()?)
//!     .migrator(migration::Migrator)
//!     .route(api::routes())
//!     .build()
//!     .await?;
//! app.run().await
//! # }
//! ```
//!
//! 测试里不启动服务，用 [`App::router`] 配合 `TestClient` 直接调路由。
//!
//! 启动顺序固定：
//!
//! 1. 检查配置；
//! 2. 连接数据库；
//! 3. `database.migrate.auto_apply = "always"` 时先执行迁移（默认不执行）；
//! 4. 装配中间件和系统路由（`/healthz`、`/readyz`、Swagger、前端静态资源）；
//! 5. 挂上业务路由，注入 [`AppState`]。

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDateTime;
use silent::middlewares::{Cors, CorsType, Logger, Timeout};
use silent::prelude::Route;
use silent::{ConnectionLimits, Listener, Server, ServerConfig};
use tokio::net::TcpListener;

use crate::config::{AppConfig, AutoApply, Env};
use crate::db::DbRegistry;
use crate::db::connect::{self, ConnectError, ConnectOptions};
use crate::db::migrate::{AnyMigrator, MigrateError, MigrationRunner, NoMigrations};
use crate::web::assets::{self, AssetSource};
use crate::web::handler::{ErrorHandler, TraceId};
use crate::web::health::{self, Readiness};

/// 启动失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    /// 配置不对。
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    /// 连不上数据库。
    #[error(transparent)]
    Connect(#[from] ConnectError),
    /// 启动时执行迁移失败。
    #[error(transparent)]
    Migrate(#[from] MigrateError),
    /// 端口监听不上。
    #[error("监听 {addr} 失败：{source}")]
    Bind {
        /// 配置里的监听地址。
        addr: String,
        /// 操作系统给出的错误，例如端口被占用。
        source: io::Error,
    },
    /// 日志初始化失败。
    #[error(transparent)]
    Logging(#[from] crate::logging::LogError),
}

/// [`App::run`] 和应用的入口函数的返回类型。
pub type BootResult = std::result::Result<(), BootError>;

/// 应用：配置、共享状态和装配好的路由。
pub struct App {
    config: Arc<AppConfig>,
    state: AppState,
    router: Route,
}

/// handler 里用 `State<AppState>` 取到的共享状态。
#[derive(Clone, Debug)]
pub struct AppState {
    /// 运行环境。
    pub env: Env,
    /// 完整的配置。
    pub config: Arc<AppConfig>,
    /// 数据库连接。
    pub db: DbRegistry,
    /// 进程启动的时间。
    pub started_at: NaiveDateTime,
    migrators: Arc<Migrators>,
    dev_reload_script: Option<String>,
}

impl AppState {
    /// 两组迁移器：`migrate` 子命令和就绪检查用。
    pub fn migrators(&self) -> &Migrators {
        &self.migrators
    }
}

/// 框架迁移器和应用迁移器。
#[derive(Debug, Clone)]
pub struct Migrators {
    /// 框架自己的迁移。
    pub framework: AnyMigrator,
    /// 应用的迁移。
    pub app: AnyMigrator,
}

impl Default for Migrators {
    fn default() -> Self {
        Self {
            framework: AnyMigrator::framework(),
            app: AnyMigrator::new(NoMigrations),
        }
    }
}

impl App {
    /// 从当前目录加载配置开始装配。
    pub fn builder() -> Result<AppBuilder, BootError> {
        Ok(AppBuilder::with_config(AppConfig::load()?))
    }

    /// 用给定的配置装配。
    pub fn with_config(config: AppConfig) -> AppBuilder {
        AppBuilder::with_config(config)
    }

    /// 配置。
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// 共享状态。
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// 装配好的路由，可以直接交给 `TestClient`。
    pub fn router(&self) -> &Route {
        &self.router
    }

    /// 按配置里的地址监听。端口写 0 时由系统分配，实际地址见 `TcpListener::local_addr`。
    pub async fn bind(&self) -> Result<TcpListener, BootError> {
        let addr: SocketAddr = self.config.server.listen.parse().map_err(|_| {
            BootError::Config(crate::config::ConfigError::Invalid(format!(
                "server.listen 不是有效的地址：{:?}",
                self.config.server.listen
            )))
        })?;
        TcpListener::bind(addr)
            .await
            .map_err(|source| BootError::Bind {
                addr: self.config.server.listen.clone(),
                source,
            })
    }

    /// 监听并一直运行，直到收到 Ctrl-C 或 SIGTERM：停止接受新连接，
    /// 在 `server.graceful_shutdown` 之内等在途请求做完，超时的连接被断开。
    pub async fn run(self) -> Result<(), BootError> {
        crate::logging::init(&self.config)?;
        let listener = self.bind().await?;
        let addr = listener
            .local_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| self.config.server.listen.clone());
        tracing::info!(
            app = %self.config.app.name,
            env = %self.config.app.env,
            "listening on http://{addr}"
        );
        self.serve(listener, std::future::pending()).await
    }

    /// 在给定的监听上运行。`shutdown` 完成时停止服务并返回。
    ///
    /// 用 `shutdown` 停止时不等在途请求：想要优雅关停就发 SIGTERM 或 Ctrl-C，由信号触发的那条路
    /// 会等 `server.graceful_shutdown` 这么久。
    pub async fn serve(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), BootError> {
        let server = Server::new()
            .listen(Listener::from(listener))
            .with_config(self.server_config())
            .with_shutdown(self.config.server.graceful_shutdown.into())
            // 监听地址由 [`App::run`] 报一次就够了。
            .on_listen(|_addrs| {});
        tokio::select! {
            _ = server.serve(self.router) => {}
            _ = shutdown => {
                tracing::info!("shutdown requested");
            }
        }
        Ok(())
    }

    fn server_config(&self) -> ServerConfig {
        let usize_max = usize::try_from(self.config.server.max_body_size.0).unwrap_or(usize::MAX);
        #[allow(clippy::needless_update)]
        ServerConfig {
            connection_limits: ConnectionLimits {
                max_body_size: Some(usize_max),
                ..ConnectionLimits::default()
            },
            ..ServerConfig::default()
        }
    }
}

/// 装配应用。
pub struct AppBuilder {
    config: AppConfig,
    migrators: Migrators,
    routes: Vec<Route>,
    #[cfg(feature = "openapi")]
    openapi: Option<silent_openapi::openapi::OpenApi>,
    assets: Option<Arc<dyn AssetSource>>,
}

impl AppBuilder {
    /// 用给定的配置装配。
    pub fn with_config(config: AppConfig) -> Self {
        Self {
            config,
            migrators: Migrators::default(),
            routes: Vec::new(),
            #[cfg(feature = "openapi")]
            openapi: None,
            assets: None,
        }
    }

    /// 换一份配置，例如测试里改监听地址。
    pub fn config(mut self, config: AppConfig) -> Self {
        self.config = config;
        self
    }

    /// 应用的迁移器。`migrate` 子命令、启动时执行迁移和 `/readyz` 都用它。
    pub fn migrator(mut self, migrator: impl Into<AnyMigrator>) -> Self {
        self.migrators.app = migrator.into();
        self
    }

    /// 挂一组业务路由，可以调用多次。
    pub fn route(mut self, route: Route) -> Self {
        self.routes.push(route);
        self
    }

    /// 提供 Swagger UI 和 `openapi.json`。挂载路径和开关见 `[openapi]`。
    #[cfg(feature = "openapi")]
    pub fn openapi(mut self, doc: silent_openapi::openapi::OpenApi) -> Self {
        self.openapi = Some(doc);
        self
    }

    /// 前端静态资源：命中不了的路径回退到 `index.html`。
    pub fn assets(mut self, source: impl AssetSource) -> Self {
        self.assets = Some(Arc::new(source));
        self
    }

    /// 连库、装配中间件和路由。不做的事：不监听端口、不初始化日志。
    pub async fn build(self) -> Result<App, BootError> {
        let Self {
            config,
            migrators,
            routes,
            #[cfg(feature = "openapi")]
            openapi,
            assets,
        } = self;
        config.validate()?;
        let production = config.app.env.is_production();
        let db = DbRegistry::new(
            connect::connect(&ConnectOptions::from_config(&config.database, production)).await?,
        );

        if matches!(config.database.migrate.auto_apply, AutoApply::Always) {
            let report = MigrationRunner::default()
                .lock_timeout(config.database.migrate.lock_timeout.into())
                .up(db.db(), &migrators.framework, &migrators.app, None)
                .await?;
            tracing::info!(
                framework = report.framework.applied.len(),
                app = report.app.applied.len(),
                "migrations applied at startup"
            );
        }

        let readiness = Arc::new(Readiness::new(
            db.clone(),
            migrators.framework.clone(),
            migrators.app.clone(),
        ));
        let state = AppState {
            env: config.app.env,
            config: Arc::new(config.clone()),
            db,
            started_at: chrono::Local::now().naive_local(),
            migrators: Arc::new(migrators),
            dev_reload_script: dev_reload_script(),
        };
        let router = assemble(
            &config,
            #[cfg(feature = "openapi")]
            openapi,
            assets,
            routes,
            state.dev_reload_script.clone(),
        );
        let router = router.with_state(state.clone()).with_state(readiness);
        Ok(App {
            config: state.config.clone(),
            state,
            router,
        })
    }
}

/// 装配中间件和路由，还没注入状态。
fn assemble(
    config: &AppConfig,
    #[cfg(feature = "openapi")] openapi: Option<silent_openapi::openapi::OpenApi>,
    assets: Option<Arc<dyn AssetSource>>,
    extra: Vec<Route>,
    dev_reload_script: Option<String>,
) -> Route {
    let mut root = Route::new_root().hook(TraceId).hook(Logger::new());
    if !config.cors.allowed_origins.is_empty() {
        root = root.hook(cors(config));
    }
    // 错误处理在压缩和超时外面：超时、handler 里的错误都经过它。
    let root = root
        .hook(ErrorHandler)
        .hook(Timeout::new(Duration::from(config.server.request_timeout)));
    #[cfg(feature = "compression")]
    let root = root.hook(silent::middlewares::Compression::default());

    // 这些前缀不归前端管：没找到文件就 404，不回退到 index.html。文档关掉时路径照样留着。
    let reserved: Vec<String> = ["api"]
        .into_iter()
        .map(str::to_owned)
        .chain(docs_prefix(config))
        .collect();

    let mut nodes = health::routes();
    #[cfg(feature = "openapi")]
    if config.openapi.is_enabled(config.app.env)
        && let Some(doc) = openapi
    {
        // 处理器按完整路径认请求，所以带上 `base_path`；节点名只用最后一段。
        let ui_path = format!("{}{}", config.app.base_path, config.openapi.path);
        match silent_openapi::SwaggerUiHandler::new(&ui_path, doc) {
            Ok(handler) => {
                let segment = config.openapi.path.trim_matches('/');
                nodes.push(swagger_node(segment, handler));
            }
            Err(e) => tracing::error!(error = %e, "Swagger UI 挂载失败"),
        }
    }
    nodes.extend(extra);
    if let Some(assets) = assets {
        let reserved: Vec<&str> = reserved.iter().map(String::as_str).collect();
        nodes.push(assets::spa_route(
            assets,
            "index.html",
            dev_reload_script,
            &reserved,
        ));
    }

    // 有 `base_path` 时节点挂在子路径那个节点下，根上只有它一个子节点。
    if config.app.base_path.is_empty() {
        nodes
            .into_iter()
            .fold(root, |route, node| route.append(node))
    } else {
        let base = nodes
            .into_iter()
            .fold(Route::new(&config.app.base_path), |route, node| {
                route.append(node)
            });
        root.append(base)
    }
}

/// 和 `SwaggerUiHandler::into_route` 相同的节点：`<segment>` 本身、`<segment>/openapi.json`、
/// `<segment>/<path:**>`，只是不再套一层空路径。`segment` 是挂载点的最后一段，例如 `swagger`。
#[cfg(feature = "openapi")]
fn swagger_node(segment: &str, handler: silent_openapi::SwaggerUiHandler) -> Route {
    use silent::prelude::{HandlerGetter, Method};
    Route::new(segment)
        .insert_handler(Method::GET, Arc::new(handler.clone()))
        .insert_handler(Method::HEAD, Arc::new(handler.clone()))
        .append(
            Route::new("<path:**>")
                .insert_handler(Method::GET, Arc::new(handler.clone()))
                .insert_handler(Method::HEAD, Arc::new(handler)),
        )
}

/// OpenAPI 的挂载路径，去掉首尾的 `/`。没启用 `openapi` feature 时没有。
#[cfg(feature = "openapi")]
fn docs_prefix(config: &AppConfig) -> Option<String> {
    Some(config.openapi.path.trim_matches('/').to_owned())
}

#[cfg(not(feature = "openapi"))]
fn docs_prefix(_config: &AppConfig) -> Option<String> {
    None
}

/// `[cors]` 配了来源才有这个中间件，见 [`AppBuilder::build`]。
fn cors(config: &AppConfig) -> Cors {
    let origins = config.cors.allowed_origins.clone();
    let mut cors = Cors::new().methods(CorsType::Any).headers(CorsType::Any);
    if origins.len() == 1 && origins[0] == "*" {
        cors = cors.origin(CorsType::Any);
    } else {
        cors = cors.origin(CorsType::AllowSome(origins));
    }
    cors.credentials(config.cors.allow_credentials)
        .max_age(3600)
}

/// `silent-boot dev` 启动应用时用 `SB_DEV_RELOAD_URL` 告诉它事件脚本的地址，只在 debug 构建里生效。
fn dev_reload_script() -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("SB_DEV_RELOAD_URL")
        .ok()
        .filter(|url| !url.is_empty())
}
