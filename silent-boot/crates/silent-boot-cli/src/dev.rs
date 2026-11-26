//! `dev` 运行循环：构建、运行应用，监听变更，重新构建并重启，再通知页面刷新。
//!
//! 这个循环放在全局工具里，不能做成应用的子命令：它要反复重启应用，
//! 不能和被重启的进程是同一个可执行文件（Windows 上没法覆盖正在运行的 exe）。
//! 它和应用是两个进程，所以重启应用时，页面连着的事件服务器不会断。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};
use tokio::sync::mpsc;

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

/// 运行循环。
pub struct DevLoop<B: Builder> {
    builder: B,
    cwd: PathBuf,
    events: Arc<Events>,
    grace: Duration,
    debounce: Duration,
    envs: Vec<(String, String)>,
    startup: Startup,
}

impl<B: Builder> DevLoop<B> {
    pub fn new(builder: B, cwd: PathBuf, events: Arc<Events>) -> Self {
        Self {
            builder,
            cwd,
            events,
            grace: Duration::from_secs(5),
            debounce: Duration::from_millis(150),
            envs: Vec::new(),
            startup: Startup::NONE,
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
        let mut shutdown = std::pin::pin!(shutdown);
        let mut child = self.start().await;
        loop {
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
                        .and_then(|running| running.try_wait().ok().flatten());
                    if let Some(status) = exited {
                        println!("[dev] 应用已退出（{status}），等下一次变更再启动");
                        child = None;
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
                    self.restart(&mut child).await;
                }
            }
        }
    }

    async fn start(&self) -> Option<Child> {
        let bin = match self.builder.build().await {
            Ok(bin) => bin,
            Err(err) => {
                eprintln!("[dev] 构建失败：\n{err}");
                self.events.publish("build-error", &err);
                return None;
            }
        };
        if !self.before_start().await {
            return None;
        }
        spawn(&bin, &self.cwd, &self.envs)
    }

    /// 第一次启动应用之前：按需执行迁移，提示还没执行的迁移。返回 `false` 表示不要启动应用。
    async fn before_start(&self) -> bool {
        let bin = self.builder.bin_path();
        if self.startup.migrate {
            let ok = run_subcommand(&bin, &self.cwd, &self.envs, &["migrate", "up"]).await;
            if !ok {
                eprintln!("[dev] 迁移没有执行成功，应用没有启动；修好迁移之后再保存一次文件");
                self.events
                    .publish("build-error", "migrate up failed, see the terminal");
                return false;
            }
        }
        if self.startup.hint_pending
            && let Some(pending) = pending_migrations(&bin, &self.cwd, &self.envs).await
        {
            println!(
                "[dev] 有 {pending} 条迁移还没执行。在另一个终端里运行 `cargo run -- migrate up`，\
                 或者改用 `silent-boot dev --migrate`，让 dev 在启动前执行"
            );
        }
        true
    }

    async fn restart(&self, child: &mut Option<Child>) {
        println!("[dev] 检测到变更，重新构建…");
        let started = Instant::now();
        if let Some(mut old) = child.take() {
            stop(&mut old, self.grace).await;
        }
        match self.builder.build().await {
            Ok(bin) => match spawn(&bin, &self.cwd, &self.envs) {
                Some(new) => {
                    *child = Some(new);
                    let elapsed = started.elapsed().as_millis();
                    println!("[dev] 已重启，从变更到重启用了 {elapsed} 毫秒");
                    self.events.publish("reload", &elapsed.to_string());
                }
                None => self
                    .events
                    .publish("build-error", &format!("启动 {} 失败", bin.display())),
            },
            Err(err) => {
                eprintln!("[dev] 构建失败：\n{err}");
                self.events.publish("build-error", &err);
            }
        }
    }
}

/// 用应用自己的子命令做一件事，输出直接打到终端。返回它是否成功。
async fn run_subcommand(bin: &Path, cwd: &Path, envs: &[(String, String)], args: &[&str]) -> bool {
    let mut command = Command::new(bin);
    command
        .current_dir(cwd)
        .args(args)
        .envs(envs.iter().map(|(k, v)| (k, v)));
    // 输出直接接到终端，迁移执行了什么一眼能看到。
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    match command.status().await {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("[dev] `{}` 以 {status} 退出", args.join(" "));
            false
        }
        Err(e) => {
            eprintln!("[dev] 运行 {} 失败：{e}", args.join(" "));
            false
        }
    }
}

/// 用 `migrate status --json` 数一数还有多少条迁移没执行。查不出来时返回 `None`，不打扰。
async fn pending_migrations(bin: &Path, cwd: &Path, envs: &[(String, String)]) -> Option<usize> {
    let output = Command::new(bin)
        .current_dir(cwd)
        .args(["migrate", "status", "--json"])
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let groups: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).ok()?;
    let pending = groups
        .iter()
        .map(|group| group["pending"].as_array().map(Vec::len).unwrap_or(0))
        .sum::<usize>();
    (pending > 0).then_some(pending)
}

/// 启动应用。标准输出和错误输出直接接到当前终端。
fn spawn(bin: &Path, cwd: &Path, envs: &[(String, String)]) -> Option<Child> {
    match Command::new(bin)
        .current_dir(cwd)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        // 循环自己被中断时，别把应用留在后台。
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("[dev] 启动 {} 失败：{e}", bin.display());
            None
        }
    }
}

/// 先发 SIGTERM 让应用优雅关停，过了 `grace` 还没退出就强杀。
async fn stop(child: &mut Child, grace: Duration) {
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
