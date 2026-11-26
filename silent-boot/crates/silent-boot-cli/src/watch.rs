//! 监听源码和配置的变更，把"有东西变了"变成一个信号。
//!
//! 只看源码和配置相关的文件；构建产物、依赖目录和文档都忽略，否则一次构建会触发下一轮构建。

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;

/// 目录监听器。丢弃时停止监听。
pub struct SourceWatcher {
    _watcher: Box<dyn notify::Watcher + Send>,
    changes: mpsc::Receiver<()>,
}

impl SourceWatcher {
    /// 递归监听 `dirs`，非递归监听 `files`。
    ///
    /// 优先用系统的文件事件；用不了（个别沙箱和挂载的文件系统会这样）就退回每 500 毫秒轮询一次。
    pub fn start(dirs: &[PathBuf], files: &[PathBuf]) -> notify::Result<Self> {
        let (tx, changes) = mpsc::channel(16);
        let signal = Signal(tx);
        let created = match watch_all(notify::recommended_watcher(signal.clone()), dirs, files) {
            Ok(watcher) => watcher,
            Err(err) => {
                eprintln!("[dev] 系统的文件事件监听不可用（{err}），改为每 500 毫秒轮询");
                let config =
                    notify::Config::default().with_poll_interval(Duration::from_millis(500));
                watch_all(notify::PollWatcher::new(signal, config), dirs, files)?
            }
        };
        Ok(Self {
            _watcher: created,
            changes,
        })
    }

    /// 变更信号。连续的变更会各发一个，合并和防抖由运行循环负责。
    pub fn changes(&mut self) -> &mut mpsc::Receiver<()> {
        &mut self.changes
    }
}

/// 建好监听器并把所有路径都加上；任何一步失败都连同监听器一起丢掉。
fn watch_all<W: notify::Watcher + Send + 'static>(
    created: notify::Result<W>,
    dirs: &[PathBuf],
    files: &[PathBuf],
) -> notify::Result<Box<dyn notify::Watcher + Send>> {
    let mut watcher: Box<dyn notify::Watcher + Send> = Box::new(created?);
    for dir in dirs {
        watcher.watch(dir, notify::RecursiveMode::Recursive)?;
    }
    for file in files {
        watcher.watch(file, notify::RecursiveMode::NonRecursive)?;
    }
    Ok(watcher)
}

/// 把 notify 的回调接到通道上。
#[derive(Clone)]
struct Signal(mpsc::Sender<()>);

impl notify::EventHandler for Signal {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        let Ok(event) = event else {
            return;
        };
        let touched = event.kind.is_create() || event.kind.is_modify() || event.kind.is_remove();
        if std::env::var_os("SB_DEV_DEBUG_WATCH").is_some() {
            eprintln!("[watch] {:?} {:?}", event.kind, event.paths);
        }
        if touched && event.paths.iter().any(|path| relevant(path)) {
            // 信号只需要传到"有变更"这一层，攒不下就丢，后面还会再来。
            let _ = self.0.try_send(());
        }
    }
}

/// 这个路径的变动要不要触发重新构建。
fn relevant(path: &Path) -> bool {
    let ignored = path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("target" | "node_modules" | ".git")
        )
    });
    if ignored {
        return false;
    }
    // 目录自身的变动不算：往目录里写一个无关文件也会改它的修改时间。
    // 目录里新增或删除源码文件时，文件自己会报事件，不会漏。
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(
            "rs" | "toml" | "html" | "tera" | "json" | "yaml" | "yml" | "css" | "js" | "ts" | "vue"
        )
    )
}
