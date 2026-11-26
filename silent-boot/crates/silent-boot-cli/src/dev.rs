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
                metadata["packages"][0]["targets"]
                    .as_array()
                    .and_then(|targets| targets.iter().find(|target| is_bin(target)))
                    .and_then(|target| target["name"].as_str())
                    .ok_or_else(|| {
                        format!(
                            "`{}` 里没有可执行目标，用 --bin 指定一个",
                            app_dir.display()
                        )
                    })?
                    .to_owned()
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

/// 运行循环。
pub struct DevLoop<B: Builder> {
    builder: B,
    cwd: PathBuf,
    events: Arc<Events>,
    grace: Duration,
    debounce: Duration,
}

impl<B: Builder> DevLoop<B> {
    pub fn new(builder: B, cwd: PathBuf, events: Arc<Events>) -> Self {
        Self {
            builder,
            cwd,
            events,
            grace: Duration::from_secs(5),
            debounce: Duration::from_millis(150),
        }
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
        match self.builder.build().await {
            Ok(bin) => spawn(&bin, &self.cwd),
            Err(err) => {
                eprintln!("[dev] 构建失败：\n{err}");
                self.events.publish("build-error", &err);
                None
            }
        }
    }

    async fn restart(&self, child: &mut Option<Child>) {
        println!("[dev] 检测到变更，重新构建…");
        let started = Instant::now();
        if let Some(mut old) = child.take() {
            stop(&mut old, self.grace).await;
        }
        match self.builder.build().await {
            Ok(bin) => match spawn(&bin, &self.cwd) {
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

/// 启动应用。标准输出和错误输出直接接到当前终端。
fn spawn(bin: &Path, cwd: &Path) -> Option<Child> {
    match Command::new(bin)
        .current_dir(cwd)
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
