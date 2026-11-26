//! 转发：在应用目录里把 `migrate`、`generate`、`healthcheck` 交给应用自己的子命令。
//!
//! 这些子命令的实现都在应用里（`silent_boot::cli`），全局工具不复制一份：
//! 迁移列表只存在于应用的 `migration` crate，实体和模块的生成器又要和应用用的 SeaORM、框架版本一致。
//! 所以全局工具做的事只有一件：找到应用根目录，执行 `cargo run --quiet -- <子命令> <参数>`，
//! 把应用的输出和退出码原样交回去。见 `docs/design.md` §6.4 和 §13.1。
//!
//! 第一次执行要编译应用，之后是增量编译。`generate` 需要应用的 `codegen` feature，转发时自动带上。

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tokio::process::Command;

/// 标记应用根目录的文件。
pub const APP_FILE: &str = "silent-boot.toml";

/// 要转发给应用的子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forward<'a> {
    /// `migrate`、`generate` 或 `healthcheck`。
    pub subcommand: &'a str,
    /// 原样交给应用子命令的参数。
    pub args: &'a [OsString],
    /// 应用有多个可执行目标时指定其中一个。
    pub bin: Option<&'a str>,
}

/// 转发失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum ForwardError {
    /// 当前目录往上找不到 [`APP_FILE`]。
    #[error(
        "从 {} 往上找不到 {APP_FILE}，这里不是 silent-boot 应用。到应用的目录里执行，或者先用 `silent-boot new` 生成一个",
        start.display()
    )]
    NotAnApp {
        /// 开始查找的目录。
        start: PathBuf,
    },
    /// cargo 没跑起来。
    #[error("运行 cargo 失败：{0}")]
    Spawn(#[from] io::Error),
}

/// 从 `start` 向上找 [`APP_FILE`]，返回它所在的目录。
pub fn app_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(APP_FILE).is_file())
        .map(Path::to_path_buf)
}

/// 交给 cargo 的参数，供测试核对。
pub fn cargo_args(forward: &Forward<'_>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["run".into(), "--quiet".into()];
    if let Some(bin) = forward.bin {
        args.push("--bin".into());
        args.push(bin.into());
    }
    // 实体和模块的生成器在应用的 `codegen` feature 里，脚手架生成的应用都带着这个 feature。
    if forward.subcommand == "generate" {
        args.extend(["--features".into(), "codegen".into()]);
    }
    args.push("--".into());
    args.push(forward.subcommand.into());
    args.extend(forward.args.iter().cloned());
    args
}

/// 在 `root` 里执行的 cargo 命令。输出接到当前的终端。
pub fn command(root: &Path, forward: &Forward<'_>) -> Command {
    let mut command = Command::new("cargo");
    command.args(cargo_args(forward)).current_dir(root);
    command
}

/// 找到应用根目录，转发，返回应用的退出码。应用被信号打断时按失败处理。
pub async fn forward(start: &Path, forward: &Forward<'_>) -> Result<ExitCode, ForwardError> {
    let root = app_root(start).ok_or(ForwardError::NotAnApp {
        start: start.to_path_buf(),
    })?;
    let shown = std::iter::once(forward.subcommand.to_owned())
        .chain(
            forward
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned()),
        )
        .collect::<Vec<_>>()
        .join(" ");
    // 第一次要编译应用，期间 cargo 不出声，先说一句在做什么。
    eprintln!(
        "[silent-boot] 在 {} 里执行 cargo run -- {shown}",
        root.display()
    );
    let status = command(&root, forward).status().await?;
    Ok(status
        .code()
        .map(|code| ExitCode::from(code as u8))
        .unwrap_or(ExitCode::FAILURE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn cargo_arguments_follow_the_convention() {
        let args = os(&["up", "--steps", "1"]);
        let forward = Forward {
            subcommand: "migrate",
            args: &args,
            bin: None,
        };
        assert_eq!(
            cargo_args(&forward),
            os(&["run", "--quiet", "--", "migrate", "up", "--steps", "1"])
        );
        let args = os(&["entity"]);
        let forward = Forward {
            subcommand: "generate",
            args: &args,
            bin: Some("demo"),
        };
        assert_eq!(
            cargo_args(&forward),
            os(&[
                "run",
                "--quiet",
                "--bin",
                "demo",
                "--features",
                "codegen",
                "--",
                "generate",
                "entity",
            ])
        );
    }

    #[test]
    fn app_root_is_found_upwards() {
        let dir = tempfile::tempdir().expect("temp dir");
        let nested = dir.path().join("src/api");
        std::fs::create_dir_all(&nested).expect("nested dir");
        assert!(app_root(&nested).is_none());
        std::fs::write(dir.path().join(APP_FILE), "[app]\nname = \"demo\"\n").expect("marker");
        assert_eq!(app_root(&nested), Some(dir.path().to_path_buf()));
        assert_eq!(app_root(dir.path()), Some(dir.path().to_path_buf()));
    }

    #[test]
    fn not_an_app_is_explained() {
        let dir = tempfile::tempdir().expect("temp dir");
        let err = ForwardError::NotAnApp {
            start: dir.path().to_path_buf(),
        };
        let text = err.to_string();
        assert!(text.contains(APP_FILE), "{text}");
        assert!(text.contains("silent-boot new"), "{text}");
    }
}
