//! `dev` 分栏面板的渲染和按键：左栏是状态和操作，右栏是日志；回退要按两次；日志跟着最新一行，翻上去之后不再跟。
//! 用 ratatui 的 TestBackend 画到内存里的缓冲区上，不需要终端。
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use silent_boot_cli::dev::{DevCommand, DevStatus, Reporter};
use silent_boot_cli::tui::{Action, Screen};
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthStr;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 24;
/// 左栏连边框的宽度，和 `tui` 里的常量一致。
const LEFT: usize = 46;

fn screen(has_migrate: bool) -> (Arc<Screen>, mpsc::Receiver<()>) {
    let (redraw, wake) = mpsc::channel(64);
    let screen = Screen::new(
        has_migrate,
        "http://127.0.0.1:9230/__dev__/reload.js".to_owned(),
        redraw,
    );
    (Arc::new(screen), wake)
}

/// 画一帧，返回每一行的文本（去掉行尾的空白）。汉字占两列，它后面那一列是占位单元，不拼进去。
fn render(screen: &Screen, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal.draw(|frame| screen.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += symbol.width().max(1) as u16;
            }
            row.trim_end().to_owned()
        })
        .collect()
}

fn text(rows: &[String]) -> String {
    rows.join("\n")
}

fn column(rows: &[String], needle: &str) -> Option<usize> {
    rows.iter().find_map(|row| row.find(needle))
}

#[test]
fn the_left_side_shows_state_and_actions_and_the_right_side_shows_the_log() {
    let (screen, mut wake) = screen(true);
    screen.note("[dev] 运行 demo，目录 /tmp/demo".to_owned());
    screen.output(
        false,
        "2026-11-27T13:46:20.566  INFO listening on http://127.0.0.1:8080".to_owned(),
    );
    screen.output(
        false,
        "\x1b[2m2026-11-27T13:46:20.566\x1b[0m \x1b[33m WARN\x1b[0m something is slow".to_owned(),
    );
    screen.status(DevStatus {
        running: Some(61384),
        busy: None,
        last_restart_ms: Some(1159),
        pending_migrations: Some(2),
        message: Some("migrate up：完成".to_owned()),
    });

    let rows = render(&screen, WIDTH, HEIGHT);
    let all = text(&rows);
    for expected in [
        "运行中 · pid 61384",
        "1159 毫秒",
        "2 条",
        "重启应用",
        "执行迁移",
        "回退一步",
        "migrate down --steps 1",
        "迁移状态",
        "清空日志",
        "migrate up：完成",
        "[dev] 运行 demo",
        "INFO listening",
        "WARN something is slow",
        "q 退出",
    ] {
        assert!(all.contains(expected), "missing {expected:?} in:\n{all}");
    }
    assert!(
        !all.contains('\u{1b}'),
        "colour codes must not reach the panel"
    );
    let actions = column(&rows, "重启应用").expect("the actions are shown");
    let log = column(&rows, "INFO listening").expect("the log is shown");
    assert!(actions < LEFT, "actions start at column {actions}");
    assert!(log >= LEFT, "the log starts at column {log}");
    // 每次有新内容都要一次重绘。
    assert!(wake.try_recv().is_ok(), "a redraw was requested");
}

#[test]
fn without_a_silent_boot_app_the_migration_actions_explain_themselves() {
    let (screen, _wake) = screen(false);
    let all = text(&render(&screen, 100, 20));
    assert!(all.contains("这个目录没有 silent-boot.toml"), "{all}");
    assert!(all.contains("没有在运行"), "{all}");
    assert!(all.contains("未知"), "{all}");
}

#[test]
fn rolling_back_needs_two_presses_and_any_other_key_cancels() {
    let (screen, _wake) = screen(true);
    let plain = KeyModifiers::empty();
    assert_eq!(
        screen.handle_key(KeyCode::Char('d'), plain),
        Action::Nothing
    );
    assert!(text(&render(&screen, 100, 20)).contains("再按一次 d 确认回退一步迁移"));

    assert_eq!(
        screen.handle_key(KeyCode::Char('x'), plain),
        Action::Nothing
    );
    assert!(!text(&render(&screen, 100, 20)).contains("再按一次 d"));

    assert_eq!(
        screen.handle_key(KeyCode::Char('d'), plain),
        Action::Nothing
    );
    assert_eq!(
        screen.handle_key(KeyCode::Char('D'), plain),
        Action::Command(DevCommand::MigrateDown { steps: 1 })
    );
    assert!(!text(&render(&screen, 100, 20)).contains("再按一次 d"));

    assert_eq!(
        screen.handle_key(KeyCode::Char('r'), plain),
        Action::Command(DevCommand::Restart)
    );
    assert_eq!(
        screen.handle_key(KeyCode::Char('m'), plain),
        Action::Command(DevCommand::MigrateUp)
    );
    assert_eq!(
        screen.handle_key(KeyCode::Char('s'), plain),
        Action::Command(DevCommand::MigrateStatus)
    );
    assert_eq!(screen.handle_key(KeyCode::Char('q'), plain), Action::Quit);
    assert_eq!(
        screen.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        Action::Quit
    );
}

#[test]
fn the_log_follows_new_lines_until_scrolled_up_and_end_returns_to_the_latest() {
    let (screen, _wake) = screen(true);
    for n in 1..=100 {
        screen.output(false, format!("row-{n:03}"));
    }
    let plain = KeyModifiers::empty();

    let bottom = text(&render(&screen, 100, 12));
    assert!(bottom.contains("row-100"), "{bottom}");
    assert!(!bottom.contains("row-001"), "{bottom}");

    let page_up = screen.handle_key(KeyCode::PageUp, plain);
    assert_eq!(page_up, Action::Scroll(20));
    screen.perform(page_up);
    let scrolled = text(&render(&screen, 100, 12));
    assert!(scrolled.contains("往上 20 行"), "{scrolled}");
    assert!(scrolled.contains("row-080"), "{scrolled}");
    assert!(!scrolled.contains("row-100"), "{scrolled}");

    // 翻上去之后来了新行：停在原处，不再跟着跑。
    screen.output(false, "row-new".to_owned());
    assert!(!text(&render(&screen, 100, 12)).contains("row-new"));

    let end = screen.handle_key(KeyCode::End, plain);
    assert_eq!(end, Action::ToBottom);
    screen.perform(end);
    let latest = text(&render(&screen, 100, 12));
    assert!(latest.contains("row-new"), "{latest}");
    assert!(latest.contains("101 行"), "{latest}");

    let clear = screen.handle_key(KeyCode::Char('c'), plain);
    assert_eq!(clear, Action::ClearLog);
    screen.perform(clear);
    assert!(!text(&render(&screen, 100, 12)).contains("row-new"));
}

#[test]
fn long_lines_wrap_by_the_width_of_the_log_panel() {
    let (screen, _wake) = screen(true);
    screen.output(false, "0123456789".repeat(20));
    let rows = render(&screen, 100, 12);
    let first = rows
        .iter()
        .find(|row| row.contains("0123456789"))
        .expect("the log is shown");
    // 一行日志从数字开始、到右边的边框为止，正好是右栏去掉两边边框的宽度。
    let start = first.find("0123456789").expect("the digits start here");
    let inside = first[start..].strip_suffix('│').unwrap_or(&first[start..]);
    assert_eq!(inside.len(), 100 - LEFT - 2);
}

#[test]
fn a_narrow_terminal_still_shows_the_actions_and_a_tiny_one_does_not_panic() {
    let (screen, _wake) = screen(true);
    screen.note("一条很长的日志，用来确认窄终端不会出问题".repeat(8));
    let narrow = text(&render(&screen, 70, 24));
    assert!(narrow.contains("重启应用"), "{narrow}");
    assert!(narrow.contains("退出"), "{narrow}");
    // 高度不够放下三个块的时候，只是少显示一些，不能崩。
    let _tiny = render(&screen, 40, 8);
    let _sliver = render(&screen, 20, 4);
}
