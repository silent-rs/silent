//! `dev` 运行循环：构建、运行应用，监听变更，重新构建并重启，再通知页面刷新。
//!
//! 这个循环放在全局工具里，不能做成应用的子命令：它要反复重启应用，
//! 不能和被重启的进程是同一个可执行文件。
//! 它和应用是两个进程，所以重启应用时，页面连着的事件服务器不会断。
//!
//! 变更之后的顺序是“先构建，构建成功才停掉旧进程”：构建没过的时候，上一次构建成功的版本
//! 接着服务，错误交给面板并盖在页面上，修好再保存一次才换上新版本。
//! 运行的是构建产物的一份副本（见 [`DevLoop::install`]）：构建期间旧进程还在跑，
//! 而 Windows 上运行中的 exe 不能被覆盖，cargo 没法把新产物写回去。
//!
//! 循环自己不碰终端：它的消息、应用和子命令的输出、状态变化都交给 [`Reporter`]。
//! 在终端里跑 `silent-boot dev` 时由分栏面板显示（见 `crate::tui`）；标准输入输出不是终端、
//! 或者带了 `--plain` 时，换成原样打印的 [`PlainReporter`]。
//! 除了文件变更，循环还接收命令（[`DevCommand`]）：手动重启、执行迁移、回退迁移、看迁移状态。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::events::Events;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 构建步骤：返回要运行的可执行文件，失败时返回要给人看的错误。
pub trait Builder: Send + Sync {
    fn build(&self) -> BoxFuture<'_, Result<PathBuf, String>>;
    /// 构建出来的可执行文件会在哪里。启动前的子命令直接运行它，不再经过 cargo。
    fn bin_path(&self) -> PathBuf;
}

/// `cargo build --bin <bin>`。
pub struct CargoBuild {
    app_dir: PathBuf,
    bin: String,
    target_dir: PathBuf,
}

impl CargoBuild {
    /// `bin` 不填时取当前包的第一个可执行目标。
    pub async fn new(app_dir: PathBuf, bin: Option<String>) -> Result<Self, String> {
        let output = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .current_dir(&app_dir)
            .stderr(Stdio::inherit())
            .output()
            .await
            .map_err(|e| format!("运行 cargo metadata 失败：{e}"))?;
        if !output.status.success() {
            return Err(format!("`{}` 不是一个 Cargo 包", app_dir.display()));
        }
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("解析 cargo metadata 的输出失败：{e}"))?;
        let target_dir = PathBuf::from(
            metadata["target_directory"]
                .as_str()
                .ok_or("cargo metadata 里没有 target_directory")?,
        );
        let bin = match bin {
            Some(bin) => bin,
            None => {
                let is_bin = |target: &serde_json::Value| {
                    target["kind"]
                        .as_array()
                        .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
                };
                let bins = |package: &serde_json::Value| -> Vec<String> {
                    package["targets"]
                        .as_array()
                        .map(|targets| {
                            targets
                                .iter()
                                .filter(|target| is_bin(target))
                                .filter_map(|target| target["name"].as_str())
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default()
                };
                // 应用的 workspace 里还有 migration、entity 这些只有库的包：先看清单就在当前目录里的那个包，
                // 再退到任何有可执行目标的包。
                let packages = metadata["packages"].as_array().cloned().unwrap_or_default();
                let root = std::fs::canonicalize(&app_dir).unwrap_or_else(|_| app_dir.clone());
                let mut packages = packages;
                packages.sort_by_key(|package| {
                    PathBuf::from(package["manifest_path"].as_str().unwrap_or_default())
                        .parent()
                        .map(|dir| dir != root)
                        .unwrap_or(true)
                });
                packages
                    .iter()
                    .filter_map(|package| bins(package).into_iter().next())
                    .next()
                    .ok_or_else(|| {
                        format!(
                            "`{}` 里没有可执行目标，用 --bin 指定一个",
                            app_dir.display()
                        )
                    })?
            }
        };
        Ok(Self {
            app_dir,
            bin,
            target_dir,
        })
    }

    /// 要运行的可执行目标的名字。
    pub fn bin(&self) -> &str {
        &self.bin
    }

    /// 构建出来的可执行文件的位置。
    pub fn bin_path(&self) -> PathBuf {
        let mut file = self.bin.clone();
        file.push_str(std::env::consts::EXE_SUFFIX);
        self.target_dir.join("debug").join(file)
    }
}

impl Builder for CargoBuild {
    fn bin_path(&self) -> PathBuf {
        CargoBuild::bin_path(self)
    }

    fn build(&self) -> BoxFuture<'_, Result<PathBuf, String>> {
        Box::pin(async move {
            let output = Command::new("cargo")
                .args(["build", "--bin", &self.bin])
                .current_dir(&self.app_dir)
                .output()
                .await
                .map_err(|e| format!("运行 cargo build 失败：{e}"))?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).to_string());
            }
            Ok(self.bin_path())
        })
    }
}

/// 启动应用之前要做的事，由应用的子命令完成（应用得是 silent-boot 应用，目录里有 `silent-boot.toml`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Startup {
    /// 启动前执行一次 `migrate up`。
    pub migrate: bool,
    /// 启动前查一次有没有待执行的迁移，有就提示对应的命令。
    pub hint_pending: bool,
}

impl Startup {
    /// 都不做。
    pub const NONE: Self = Self {
        migrate: false,
        hint_pending: false,
    };
    /// 只提示，不代劳。
    pub const HINT: Self = Self {
        migrate: false,
        hint_pending: true,
    };
    /// 启动前执行一次迁移。
    pub const MIGRATE: Self = Self {
        migrate: true,
        hint_pending: true,
    };
}

/// 循环对外说话的出口。实现在收到一行的时候记下来就行，不用管顺序以外的事。
pub trait Reporter: Send + Sync + 'static {
    /// 循环自己的消息，一行一条。
    fn note(&self, line: String);
    /// 应用或子命令的输出，一行一条。`stderr` 表示这一行来自标准错误。
    fn output(&self, stderr: bool, line: String);
    /// 状态变了。
    fn status(&self, status: DevStatus);
}

/// 原样打印：循环的消息和应用的标准输出走标准输出，标准错误走标准错误。不显示状态。
pub struct PlainReporter;

impl Reporter for PlainReporter {
    fn note(&self, line: String) {
        println!("{line}");
    }

    fn output(&self, stderr: bool, line: String) {
        if stderr {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }

    fn status(&self, _status: DevStatus) {}
}

/// 面板要显示的状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevStatus {
    /// 应用进程的 PID；没在运行时是 `None`。
    pub running: Option<u32>,
    /// 正在做的事，例如“构建中”；空闲时是 `None`。
    pub busy: Option<String>,
    /// 上一次重启用了多少毫秒。
    pub last_restart_ms: Option<u128>,
    /// 还没执行的迁移有几条；查不出来时是 `None`。
    pub pending_migrations: Option<usize>,
    /// 最近一条命令的结果，给面板显示。
    pub message: Option<String>,
}

/// 面板（或者别的调用方）能让循环做的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevCommand {
    /// 重新构建并重启。
    Restart,
    /// 执行待执行的迁移：`migrate up`。
    MigrateUp,
    /// 回退最后执行的几条应用迁移：`migrate down --steps N`。
    MigrateDown { steps: u32 },
    /// 把两组迁移的状态打进日志：`migrate status`。
    MigrateStatus,
}

/// 跑起来的应用：进程本身，加上两个把它的输出送进日志的任务。
struct Running {
    child: Child,
    // 只为了持有它们：应用退出、输出读完之后它们自己结束；循环结束时和进程一起丢掉。
    _pumps: Vec<JoinHandle<()>>,
}

/// 运行循环。
pub struct DevLoop<B: Builder> {
    builder: B,
    cwd: PathBuf,
    events: Arc<Events>,
    reporter: Arc<dyn Reporter>,
    grace: Duration,
    debounce: Duration,
    envs: Vec<(String, String)>,
    startup: Startup,
    /// 应用是不是 silent-boot 应用：只有它才有 `migrate` 子命令，才接受迁移相关的命令。
    migrate_commands: bool,
    status: DevStatus,
}

impl<B: Builder> DevLoop<B> {
    /// 消息原样打印。要用面板显示时用 [`DevLoop::with_reporter`]。
    pub fn new(builder: B, cwd: PathBuf, events: Arc<Events>) -> Self {
        Self::with_reporter(builder, cwd, events, Arc::new(PlainReporter))
    }

    pub fn with_reporter(
        builder: B,
        cwd: PathBuf,
        events: Arc<Events>,
        reporter: Arc<dyn Reporter>,
    ) -> Self {
        Self {
            builder,
            cwd,
            events,
            reporter,
            grace: Duration::from_secs(5),
            debounce: Duration::from_millis(150),
            envs: Vec::new(),
            startup: Startup::NONE,
            migrate_commands: false,
            status: DevStatus::default(),
        }
    }

    /// 传给应用进程的环境变量。
    pub fn envs(mut self, envs: Vec<(String, String)>) -> Self {
        self.envs = envs;
        self
    }

    /// 第一次构建成功之后、启动应用之前要做的事。
    pub fn startup(mut self, startup: Startup) -> Self {
        self.startup = startup;
        self
    }

    /// 应用有 `migrate` 子命令（目录里有 `silent-boot.toml`）时才接受迁移相关的命令、才去数待执行的迁移。
    pub fn migrate_commands(mut self, yes: bool) -> Self {
        self.migrate_commands = yes;
        self
    }

    /// 停掉旧进程时，先发 SIGTERM，过了这段时间还没退出就强杀。
    pub fn grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// 收到第一个变更信号后，再等这么久才开始构建，把连续保存的几个文件并成一次。
    pub fn debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    /// 一直运行到 `shutdown` 完成。返回前会停掉应用进程。
    pub async fn run(self, changes: &mut mpsc::Receiver<()>, shutdown: impl Future<Output = ()>) {
        self.drive(changes, None, shutdown).await;
    }

    /// 同 [`DevLoop::run`]，另外接收命令。发端被丢掉之后循环照常运行，只是不再有命令。
    pub async fn run_with_commands(
        self,
        changes: &mut mpsc::Receiver<()>,
        commands: mpsc::Receiver<DevCommand>,
        shutdown: impl Future<Output = ()>,
    ) {
        self.drive(changes, Some(commands), shutdown).await;
    }

    async fn drive(
        mut self,
        changes: &mut mpsc::Receiver<()>,
        commands: Option<mpsc::Receiver<DevCommand>>,
        shutdown: impl Future<Output = ()>,
    ) {
        let mut shutdown = std::pin::pin!(shutdown);
        let mut commands = commands;
        let mut commands_closed = commands.is_none();
        let mut child = self.start().await;
        loop {
            // 没有命令通道（或者它已经关了）时这个 future 永远不结束，分支也不会被选中。
            let next_command = async {
                match commands.as_mut() {
                    Some(commands) => commands.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = &mut shutdown => {
                    if let Some(mut child) = child.take() {
                        stop(&mut child, self.grace).await;
                    }
                    return;
                }
                // 应用自己退出时（比如崩溃），把子进程清掉，等下一次变更再启动。
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    let exited = child
                        .as_mut()
                        .and_then(|running| running.child.try_wait().ok().flatten());
                    if let Some(status) = exited {
                        self.note(format!("[dev] 应用已退出（{status}），等下一次变更再启动"));
                        child = None;
                        self.status.running = None;
                        self.publish_status();
                    }
                }
                changed = changes.recv() => {
                    if changed.is_none() {
                        if let Some(mut child) = child.take() {
                            stop(&mut child, self.grace).await;
                        }
                        return;
                    }
                    tokio::time::sleep(self.debounce).await;
                    while changes.try_recv().is_ok() {}
                    self.restart(&mut child, "检测到变更").await;
                }
                command = next_command, if !commands_closed => {
                    match command {
                        Some(command) => self.execute(command, &mut child).await,
                        None => commands_closed = true,
                    }
                }
            }
        }
    }

    async fn start(&mut self) -> Option<Running> {
        self.set_busy(Some("构建中".to_owned()));
        let built = match self.builder.build().await {
            Ok(built) => built,
            Err(err) => {
                self.set_busy(None);
                self.note(format!("[dev] 构建失败：\n{err}"));
                self.events.publish("build-error", &err);
                return None;
            }
        };
        if !self.before_start().await {
            self.set_busy(None);
            return None;
        }
        let bin = Self::install(&built);
        let running = spawn(&bin, &self.cwd, &self.envs, &self.reporter);
        self.status.running = running.as_ref().and_then(|running| running.child.id());
        self.set_busy(None);
        running
    }

    /// 第一次启动应用之前：按需执行迁移，数一数待执行的迁移，有就提示对应的命令。返回 `false` 表示不要启动应用。
    async fn before_start(&mut self) -> bool {
        let bin = self.builder.bin_path();
        if self.startup.migrate {
            self.set_busy(Some("migrate up".to_owned()));
            let ok = run_subcommand(
                &bin,
                &self.cwd,
                &self.envs,
                &["migrate", "up"],
                &self.reporter,
            )
            .await;
            if !ok {
                self.note(
                    "[dev] 迁移没有执行成功，应用没有启动；修好迁移之后再保存一次文件".to_owned(),
                );
                self.events
                    .publish("build-error", "migrate up failed, see the terminal");
                return false;
            }
        }
        if self.migrate_commands {
            self.set_busy(Some("查询迁移状态".to_owned()));
            self.status.pending_migrations = pending_migrations(&bin, &self.cwd, &self.envs).await;
            self.set_busy(None);
            let pending = self.status.pending_migrations.unwrap_or(0);
            if self.startup.hint_pending && pending > 0 {
                self.note(format!(
                    "[dev] 有 {pending} 条迁移还没执行。按 m 执行，或者改用 `silent-boot dev --migrate`，让 dev 在启动前执行"
                ));
            }
        }
        true
    }

    /// 执行一条命令。循环在它做完之前不接收别的命令，所以不用防重入。
    async fn execute(&mut self, command: DevCommand, child: &mut Option<Running>) {
        match command {
            DevCommand::Restart => self.restart(child, "手动重启").await,
            DevCommand::MigrateUp => self.migrate(&["migrate", "up"]).await,
            DevCommand::MigrateDown { steps } => {
                let steps = steps.to_string();
                self.migrate(&["migrate", "down", "--steps", &steps]).await
            }
            DevCommand::MigrateStatus => self.migrate(&["migrate", "status"]).await,
        }
    }

    /// 停掉旧进程之前先把新的构建出来：构建没过的时候，上一次的版本接着服务。
    async fn restart(&mut self, child: &mut Option<Running>, cause: &str) {
        self.note(format!("[dev] {cause}，重新构建…"));
        self.set_busy(Some("构建中".to_owned()));
        let started = Instant::now();
        let built = match self.builder.build().await {
            Ok(built) => built,
            Err(err) => {
                self.set_busy(None);
                self.note(format!("[dev] 构建失败：\n{err}"));
                let kept = child.is_some();
                if kept {
                    self.note("[dev] 上一次构建成功的版本还在运行，修好之后再保存一次".to_owned());
                }
                self.events
                    .publish("build-error", &Self::build_error(&err, kept));
                return;
            }
        };
        if let Some(mut old) = child.take() {
            stop(&mut old, self.grace).await;
        }
        let bin = Self::install(&built);
        match spawn(&bin, &self.cwd, &self.envs, &self.reporter) {
            Some(new) => {
                *child = Some(new);
                let elapsed = started.elapsed().as_millis();
                self.note(format!("[dev] 已重启，用了 {elapsed} 毫秒"));
                self.status.last_restart_ms = Some(elapsed);
                self.events.publish("reload", &elapsed.to_string());
            }
            None => self
                .events
                .publish("build-error", &format!("启动 {} 失败", bin.display())),
        }
        self.status.running = child.as_ref().and_then(|running| running.child.id());
        self.set_busy(None);
    }

    /// 在应用旁边跑应用的 `migrate …` 子命令，输出进日志；之后重新数一遍待执行的迁移。
    /// 应用不停：表结构变了的话要按 `r` 重启才会用上新结构。
    async fn migrate(&mut self, args: &[&str]) {
        let shown = args.join(" ");
        if !self.migrate_commands {
            self.note(format!(
                "[dev] 这里不是 silent-boot 应用，没有 `{shown}` 子命令"
            ));
            self.status.message = Some(format!("这个目录没有 `{shown}`"));
            self.publish_status();
            return;
        }
        self.set_busy(Some(shown.clone()));
        let bin = self.builder.bin_path();
        let ok = run_subcommand(&bin, &self.cwd, &self.envs, args, &self.reporter).await;
        if ok {
            self.note(format!("[dev] `{shown}` 执行完"));
            if shown.starts_with("migrate up") || shown.starts_with("migrate down") {
                self.note("[dev] 表结构变了的话按 r 重启，应用才会用上新的实体".to_owned());
            }
        } else {
            self.note(format!("[dev] `{shown}` 没有成功，看上面的输出"));
        }
        self.status.message = Some(if ok {
            format!("{shown}：完成")
        } else {
            format!("{shown}：失败")
        });
        self.set_busy(None);
        self.refresh_pending().await;
    }

    async fn refresh_pending(&mut self) {
        if !self.migrate_commands {
            return;
        }
        self.set_busy(Some("查询迁移状态".to_owned()));
        let bin = self.builder.bin_path();
        self.status.pending_migrations = pending_migrations(&bin, &self.cwd, &self.envs).await;
        self.set_busy(None);
    }

    fn set_busy(&mut self, busy: Option<String>) {
        self.status.busy = busy;
        self.publish_status();
    }

    fn publish_status(&self) {
        self.reporter.status(self.status.clone());
    }

    /// 循环自己的消息。多行的（编译错误）拆成一行一条，面板里好滚。
    fn note(&self, line: impl Into<String>) {
        for piece in line.into().split('\n') {
            self.reporter.note(piece.to_owned());
        }
    }

    /// 盖在页面上的构建错误。上一次的版本还在服务时先说一句，免得以为页面已经不能用了。
    fn build_error(err: &str, kept_running: bool) -> String {
        if kept_running {
            format!("构建没通过，页面现在由上一次构建成功的版本提供服务。\n\n{err}")
        } else {
            err.to_owned()
        }
    }

    /// 把构建产物复制一份，返回要运行的副本。
    ///
    /// 不直接运行 `cargo build` 的产物：构建期间旧进程还在跑，而 Windows 上运行中的 exe
    /// 不能被覆盖，cargo 写不回去。macOS 和 Linux 上 cargo 是先删掉旧文件再放新的，旧进程
    /// 不受影响（实测见 `docs/m1-verification.md` 的 F29），副本是为了各个平台走同一条路。
    /// 副本放在产物旁边的 `.silent-boot-dev/` 里，先写到临时名字再改名，不会留下复制了一半的文件。
    /// 复制不了（比如副本还被上一次没退干净的进程占着）就退回运行产物本身。
    fn install(built: &Path) -> PathBuf {
        let (Some(dir), Some(name)) = (built.parent(), built.file_name()) else {
            return built.to_path_buf();
        };
        let dir = dir.join(".silent-boot-dev");
        let copy = dir.join(name);
        let staging = dir.join(format!(".{}.new", name.to_string_lossy()));
        let installed = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::copy(built, &staging))
            .and_then(|_| std::fs::rename(&staging, &copy));
        match installed {
            Ok(()) => copy,
            Err(err) => {
                eprintln!(
                    "[dev] 复制构建产物失败（{err}），直接运行 {}",
                    built.display()
                );
                built.to_path_buf()
            }
        }
    }
}

/// 用应用自己的子命令做一件事，输出进日志。返回它是否成功。
///
/// 等子命令结束再一起交出去：这些输出都不长，先输出后错误的顺序反而看得更清楚。
async fn run_subcommand(
    bin: &Path,
    cwd: &Path,
    envs: &[(String, String)],
    args: &[&str],
    reporter: &Arc<dyn Reporter>,
) -> bool {
    let mut command = Command::new(bin);
    command
        .current_dir(cwd)
        .args(args)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = match command.output().await {
        Ok(output) => output,
        Err(e) => {
            reporter.note(format!("[dev] 运行 {} 失败：{e}", args.join(" ")));
            return false;
        }
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        reporter.output(false, line.to_owned());
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        reporter.output(true, line.to_owned());
    }
    if !output.status.success() {
        reporter.note(format!(
            "[dev] `{}` 以 {} 退出",
            args.join(" "),
            output.status
        ));
        return false;
    }
    true
}

/// 用 `migrate status --json` 数还有多少条迁移没执行。查不出来时返回 `None`，不打扰。
async fn pending_migrations(bin: &Path, cwd: &Path, envs: &[(String, String)]) -> Option<usize> {
    let output = Command::new(bin)
        .current_dir(cwd)
        .args(["migrate", "status", "--json"])
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let groups: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).ok()?;
    Some(
        groups
            .iter()
            .map(|group| group["pending"].as_array().map(Vec::len).unwrap_or(0))
            .sum(),
    )
}

/// 启动应用：它的输出不直接写终端，一行一行交给 `reporter`。
fn spawn(
    bin: &Path,
    cwd: &Path,
    envs: &[(String, String)],
    reporter: &Arc<dyn Reporter>,
) -> Option<Running> {
    let mut command = Command::new(bin);
    command
        .current_dir(cwd)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        // 循环自己被中断时，别把应用留在后台。
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            reporter.note(format!("[dev] 启动 {} 失败：{e}", bin.display()));
            return None;
        }
    };
    let mut pumps = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        pumps.push(tokio::spawn(pump(stdout, false, Arc::clone(reporter))));
    }
    if let Some(stderr) = child.stderr.take() {
        pumps.push(tokio::spawn(pump(stderr, true, Arc::clone(reporter))));
    }
    Some(Running {
        child,
        _pumps: pumps,
    })
}

async fn pump<R: AsyncRead + Unpin>(reader: R, stderr: bool, reporter: Arc<dyn Reporter>) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        reporter.output(stderr, line);
    }
}

/// 先发 SIGTERM 让应用优雅关停，过了 `grace` 还没退出就强杀。
async fn stop(running: &mut Running, grace: Duration) {
    let child = &mut running.child;
    if let Some(pid) = child.id() {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    if tokio::time::timeout(grace, child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
}
