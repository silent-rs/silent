//! `dev` 的分栏面板：左边是状态和操作，右边是日志。
//!
//! 面板只做两件事：把 [`DevStatus`] 和日志画出来；把按键翻译成给运行循环的命令（[`on_key`]）。
//! 面板一侧的 [`Reporter`] 是 [`Screen`]，终端的进入和退出、事件循环在 [`run`] 里。
//! 应用和子命令的输出一行一行进 [`LogBuffer`]，按面板当前的宽度预先折行，所以滚动和“跟到最新”都按行算得准。

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use unicode_width::UnicodeWidthChar;

use crate::dev::{DevCommand, DevStatus, Reporter};

/// 左栏的宽度（连边框）。
const LEFT_WIDTH: u16 = 46;
/// 日志最多留多少行。
const LOG_CAPACITY: usize = 2000;
/// 一次翻页翻多少行。
const PAGE: usize = 20;

/// 一行日志的来路，决定颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// 循环自己的消息。
    Dev,
    /// 应用的标准输出。
    App,
    /// 标准错误、`WARN`。
    Warning,
    /// `ERROR`、panic。
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogLine {
    text: String,
    tone: Tone,
}

/// 日志缓冲：只留最近的一些行。`lift` 是从底部往上抬了多少行，0 表示贴着最新的一行。
#[derive(Debug)]
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    lift: usize,
}

impl LogBuffer {
    fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            lift: 0,
        }
    }

    fn push(&mut self, text: &str, tone: Tone) {
        for line in text.split('\n') {
            if self.lines.len() == LOG_CAPACITY {
                self.lines.pop_front();
            }
            self.lines.push_back(LogLine {
                // 不管是谁推进来的，面板里只放纯文本，颜色由 tone 决定。
                text: strip_ansi(line),
                tone,
            });
        }
    }

    fn clear(&mut self) {
        self.lines.clear();
        self.lift = 0;
    }

    fn scroll(&mut self, up: isize) {
        let max = self.rows().saturating_sub(1);
        let lift = self.lift as isize + up;
        self.lift = lift.clamp(0, max as isize) as usize;
    }

    fn follow_latest(&mut self) {
        self.lift = 0;
    }

    /// 离最新的一行有多远，0 表示贴着最新的一行。
    fn lift(&self) -> usize {
        self.lift
    }

    /// 折行之后一共多少行。
    fn rows(&self) -> usize {
        self.view(usize::MAX).len()
    }

    /// 按面板宽度折行之后的所有行。
    fn view(&self, width: usize) -> Vec<LogLine> {
        self.lines
            .iter()
            .flat_map(|line| {
                wrap(&line.text, width)
                    .into_iter()
                    .map(move |text| LogLine {
                        text,
                        tone: line.tone,
                    })
            })
            .collect()
    }
}

/// 按显示宽度折行：中文占两列。空行原样保留一行。
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let wide = ch.width().unwrap_or(0);
        if used + wide > width && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push(ch);
        used += wide;
    }
    rows.push(row);
    rows
}

/// 去掉 ANSI 转义序列：应用的日志带着颜色码，面板自己上色。
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            // CSI：参数字节之后以一个 0x40..=0x7e 的字符结尾，例如 `ESC[32m`、`ESC[2J`。
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&next) {
                        break;
                    }
                }
            }
            // OSC：到 BEL 或者 `ESC \` 为止，例如改窗口标题。
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' {
                        chars.next();
                        break;
                    }
                }
            }
            // 其它的转义序列：转义符后面只跟一个字符。
            _ => {}
        }
    }
    out
}

fn tone_of(stderr: bool, line: &str) -> Tone {
    // tracing 的 pretty 格式里级别出现在行首：`2026-11-27T13:46:20.566  INFO accepted connection …`。
    let head: String = line
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    if head.contains(" ERROR") || line.contains("panicked at") {
        Tone::Error
    } else if stderr || head.contains(" WARN") {
        Tone::Warning
    } else {
        Tone::App
    }
}

/// 面板这一侧的状态：运行循环写进来，面板读出去画。
pub struct Screen {
    inner: Mutex<Panel>,
    redraw: Mutex<mpsc::Sender<()>>,
    /// 这个目录的应用有没有 `migrate` 子命令。
    has_migrate: bool,
}

#[derive(Debug)]
struct Panel {
    status: DevStatus,
    log: LogBuffer,
    /// 用户按了一次 `d`，等第二次确认。
    confirming: bool,
    /// 页面刷新脚本的地址，提示给开发者。
    reload_url: String,
}

impl Screen {
    pub fn new(has_migrate: bool, reload_url: String, redraw: mpsc::Sender<()>) -> Self {
        Self {
            inner: Mutex::new(Panel {
                status: DevStatus::default(),
                log: LogBuffer::new(),
                confirming: false,
                reload_url,
            }),
            redraw: Mutex::new(redraw),
            has_migrate,
        }
    }

    fn update<T>(&self, change: impl FnOnce(&mut Panel) -> T) -> T {
        let out = change(
            &mut self
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        // 攒不住就说明已经有一次重绘在排队了。
        let _ = self
            .redraw
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .try_send(());
        out
    }
}

impl Reporter for Screen {
    fn note(&self, line: String) {
        self.update(|panel| panel.log.push(&line, Tone::Dev));
    }

    fn output(&self, stderr: bool, line: String) {
        let line = strip_ansi(&line);
        let tone = tone_of(stderr, &line);
        self.update(|panel| panel.log.push(&line, tone));
    }

    fn status(&self, status: DevStatus) {
        self.update(|panel| panel.status = status);
    }
}

impl Screen {
    /// 画一帧。
    pub fn draw(&self, frame: &mut ratatui::Frame<'_>) {
        let panel = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        draw(frame, &panel, self.has_migrate);
    }

    /// 循环正在做上一件事：这时来的命令先不交过去，在提示里说明。
    pub fn is_busy(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .status
            .busy
            .is_some()
    }

    /// 按键到动作。“再按一次确认”的状态在这里维护，返回的是真正要执行的动作。
    pub fn handle_key(&self, code: KeyCode, mods: KeyModifiers) -> Action {
        self.update(|panel| {
            let action = on_key(code, mods, panel.confirming);
            match action {
                Action::AskConfirm => {
                    panel.confirming = true;
                    return Action::Nothing;
                }
                Action::CancelConfirm => {
                    panel.confirming = false;
                    return Action::Nothing;
                }
                Action::Command(_) => panel.confirming = false,
                _ => {}
            }
            action
        })
    }

    /// 执行不用交给循环的动作：滚动、清空。命令和退出由事件循环处理。
    pub fn perform(&self, action: Action) {
        match action {
            Action::Scroll(up) => self.update(|panel| panel.log.scroll(up)),
            Action::ToBottom => self.update(|panel| panel.log.follow_latest()),
            Action::ClearLog => self.update(|panel| panel.log.clear()),
            _ => {}
        }
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, panel: &Panel, has_migrate: bool) {
    let area = frame.area();
    let split = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    let (main, footer) = (split[0], split[1]);
    let split =
        Layout::horizontal([Constraint::Length(LEFT_WIDTH), Constraint::Min(0)]).split(main);
    let (left, right) = (split[0], split[1]);

    let state_rows: u16 = 7;
    let hint_rows: u16 = 6;
    let split = Layout::vertical([
        Constraint::Length(state_rows),
        Constraint::Min(0),
        Constraint::Length(hint_rows),
    ])
    .split(left);
    let (state, actions, hint) = (split[0], split[1], split[2]);

    let status = &panel.status;
    let dim = Style::new().fg(Color::DarkGray);
    let mut state_lines = vec![
        labeled("应用", running_text(status), dim),
        labeled(
            "正在",
            status.busy.clone().unwrap_or_else(|| "—".to_owned()),
            dim,
        ),
        labeled(
            "上次重启",
            status
                .last_restart_ms
                .map_or_else(|| "—".to_owned(), |ms| format!("{ms} 毫秒")),
            dim,
        ),
        labeled(
            "待执行迁移",
            status
                .pending_migrations
                .map_or_else(|| "未知".to_owned(), |n| format!("{n} 条")),
            dim,
        ),
    ];
    state_lines.push(Line::from(""));
    state_lines.push(labeled("页面脚本", panel.reload_url.clone(), dim));
    frame.render_widget(
        Paragraph::new(Text::from(state_lines)).block(bordered(" 状态 ")),
        state,
    );

    let unavailable = !has_migrate;
    let item = |key: &'static str, name: &'static str, command: &'static str, muted: bool| {
        let style = if muted { dim } else { Style::new() };
        Line::from(vec![
            Span::styled(format!(" {key:<4}"), style.fg(Color::Cyan)),
            Span::styled(format!("{name}  "), style),
            Span::styled(command, dim),
        ])
    };
    let mut action_lines = vec![
        item("r", "重启应用", "重新构建并重启", false),
        item("m", "执行迁移", "migrate up", unavailable),
        item("d", "回退一步", "migrate down --steps 1", unavailable),
        item("s", "迁移状态", "migrate status", unavailable),
        item("c", "清空日志", "", false),
        item("q", "退出", "停掉应用，退出面板", false),
    ];
    action_lines.push(Line::from(""));
    action_lines.push(Line::from(Span::styled(
        " ↑↓ PgUp PgDn  滚动日志，End 跟到最新",
        dim,
    )));
    frame.render_widget(
        Paragraph::new(Text::from(action_lines)).block(bordered(" 操作 ")),
        actions,
    );

    let mut hint_lines = Vec::new();
    if panel.confirming {
        hint_lines.push(Line::from(Span::styled(
            " 再按一次 d 确认回退一步迁移，按别的键取消",
            Style::new()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    } else if unavailable {
        hint_lines.push(Line::from(Span::styled(
            " 这个目录没有 silent-boot.toml，迁移相关的操作不可用",
            dim,
        )));
    }
    if let Some(busy) = &status.busy {
        hint_lines.push(Line::from(Span::styled(
            format!(" 正在：{busy}"),
            Style::new().fg(Color::Yellow),
        )));
    }
    if let Some(message) = &status.message {
        hint_lines.push(Line::from(Span::raw(format!(" {message}"))));
    }
    frame.render_widget(
        Paragraph::new(Text::from(hint_lines)).block(bordered(" 提示 ")),
        hint,
    );

    // 日志：先折行，再按“贴着底部、往上抬 lift 行”截出一屏。
    let inner_width = right.width.saturating_sub(2) as usize;
    let rows = panel.log.view(inner_width);
    let height = right.height.saturating_sub(2) as usize;
    let top = rows
        .len()
        .saturating_sub(height)
        .saturating_sub(panel.log.lift());
    let visible: Vec<Line> = rows
        .iter()
        .skip(top)
        .take(height)
        .map(|row| {
            let color = match row.tone {
                Tone::Dev => Color::Cyan,
                Tone::App => Color::Reset,
                Tone::Warning => Color::Yellow,
                Tone::Error => Color::Red,
            };
            Line::from(Span::styled(row.text.clone(), Style::new().fg(color)))
        })
        .collect();
    let title = if panel.log.lift() == 0 {
        format!(" 日志（{} 行） ", rows.len())
    } else {
        format!(" 日志（往上 {} 行，End 回到最新） ", panel.log.lift())
    };
    frame.render_widget(
        Paragraph::new(Text::from(visible)).block(bordered(&title)),
        right,
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " silent-boot dev   r 重启 · m 迁移 · d 回退 · s 状态 · c 清空 · ↑↓ 滚动 · q 退出",
            dim,
        ))),
        footer,
    );
}

fn bordered(title: &str) -> Block<'_> {
    Block::bordered()
        .title(title)
        .border_style(Style::new().fg(Color::DarkGray))
}

fn labeled(name: &'static str, value: String, dim: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {name:<10}"), dim),
        Span::raw(value),
    ])
}

fn running_text(status: &DevStatus) -> String {
    match status.running {
        Some(pid) => format!("运行中 · pid {pid}"),
        None => "没有在运行".to_owned(),
    }
}

/// 按一个键之后面板要做的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Command(DevCommand),
    /// 向上滚（负数向下）。
    Scroll(isize),
    ToBottom,
    ClearLog,
    /// 进入“再按一次确认”。
    AskConfirm,
    CancelConfirm,
    Quit,
}

/// 按键到动作。`confirming` 时只认第二次 `d`（确认）和别的键（取消）。
pub fn on_key(code: KeyCode, mods: KeyModifiers, confirming: bool) -> Action {
    if confirming {
        return match code {
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Enter => {
                Action::Command(DevCommand::MigrateDown { steps: 1 })
            }
            _ => Action::CancelConfirm,
        };
    }
    if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        return Action::Quit;
    }
    match code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('r') => Action::Command(DevCommand::Restart),
        KeyCode::Char('m') => Action::Command(DevCommand::MigrateUp),
        KeyCode::Char('d') => Action::AskConfirm,
        KeyCode::Char('s') => Action::Command(DevCommand::MigrateStatus),
        KeyCode::Char('c') => Action::ClearLog,
        KeyCode::Up => Action::Scroll(1),
        KeyCode::Down => Action::Scroll(-1),
        KeyCode::PageUp => Action::Scroll(PAGE as isize),
        KeyCode::PageDown => Action::Scroll(-(PAGE as isize)),
        KeyCode::End => Action::ToBottom,
        _ => Action::Nothing,
    }
}

/// 终端那边的变化，由 [`spawn_event_reader`] 送过来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermEvent {
    Key(KeyCode, KeyModifiers),
    Resize,
}

/// 在阻塞线程里读终端事件。`stop` 置位之后最多再等 200 毫秒就退出，不挡着进程结束。
pub fn spawn_event_reader(tx: mpsc::Sender<TermEvent>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        while !stop.load(Ordering::SeqCst) {
            let ready = match crossterm::event::poll(Duration::from_millis(200)) {
                Ok(ready) => ready,
                Err(_) => break,
            };
            if !ready {
                continue;
            }
            let event = match crossterm::event::read() {
                Ok(event) => event,
                Err(_) => break,
            };
            let sent = match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    tx.blocking_send(TermEvent::Key(key.code, key.modifiers))
                }
                Event::Resize(_, _) => tx.blocking_send(TermEvent::Resize),
                _ => Ok(()),
            };
            if sent.is_err() {
                break;
            }
        }
    })
}

/// 面板的事件循环：有变化就重画，把按键翻译成命令交给循环。`interrupt` 结束时退出（比如 SIGTERM）。
/// 退出之前把终端恢复原样。
pub async fn run(
    screen: Arc<Screen>,
    commands: mpsc::Sender<DevCommand>,
    mut events: mpsc::Receiver<TermEvent>,
    mut redraw: mpsc::Receiver<()>,
    mut interrupt: oneshot::Receiver<()>,
) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let outcome = loop {
        terminal.draw(|frame| screen.draw(frame))?;
        tokio::select! {
            event = events.recv() => match event {
                None => break Ok(()),
                Some(TermEvent::Resize) => {}
                Some(TermEvent::Key(code, mods)) => {
                    let busy = screen.is_busy();
                    let action = screen.handle_key(code, mods);
                    match action {
                        Action::Nothing => {}
                        Action::Quit => break Ok(()),
                        Action::Command(command) => {
                            if busy {
                                screen.update(|panel| {
                                    panel.status.message = Some(format!(
                                        "{}：上一件事还没做完，等它结束再按",
                                        describe(&command)
                                    ))
                                });
                            } else if commands.send(command).await.is_err() {
                                break Ok(());
                            }
                        }
                        Action::Scroll(_) | Action::ToBottom | Action::ClearLog => screen.perform(action),
                        Action::AskConfirm | Action::CancelConfirm => {}
                    }
                }
            },
            _ = redraw.recv() => {}
            _ = &mut interrupt => break Ok(()),
        }
    };
    ratatui::restore();
    outcome
}

fn describe(command: &DevCommand) -> &'static str {
    match command {
        DevCommand::Restart => "重启",
        DevCommand::MigrateUp => "执行迁移",
        DevCommand::MigrateDown { .. } => "回退迁移",
        DevCommand::MigrateStatus => "迁移状态",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_of(text: &str) -> String {
        let mut log = LogBuffer::new();
        log.push(text, Tone::App);
        log.view(80).remove(0).text
    }

    #[test]
    fn ansi_sequences_are_stripped_and_plain_text_kept() {
        assert_eq!(strip_ansi("\x1b[32m INFO\x1b[0m ok"), " INFO ok");
        assert_eq!(strip_ansi("\x1b]0;title\x07after"), "after");
        assert_eq!(strip_ansi("没有转义"), "没有转义");
        assert_eq!(row_of("\x1b[2mspan\x1b[0m"), "span");
    }

    #[test]
    fn long_lines_wrap_by_display_width_and_short_ones_stay() {
        assert_eq!(wrap("abc def", 10), vec!["abc def"]);
        assert_eq!(wrap("abc def", 3), vec!["abc", " de", "f"]);
        // 中文占两列：宽度 6 正好放下三个汉字，第四个换到下一行。
        assert_eq!(wrap("日志折行测试", 6), vec!["日志折", "行测试"]);
        assert_eq!(wrap("", 6), vec![""]);
    }

    #[test]
    fn scrolling_is_clamped_and_follows_new_lines_only_at_the_bottom() {
        let mut log = LogBuffer::new();
        for n in 0..10 {
            log.push(&n.to_string(), Tone::App);
        }
        assert_eq!(log.lift(), 0);
        log.scroll(3);
        assert_eq!(log.lift(), 3);
        log.scroll(-5);
        assert_eq!(log.lift(), 0, "scrolling down stops at the newest line");
        log.scroll(100);
        assert_eq!(log.lift(), 9, "scrolling up stops at the first line");
        log.clear();
        assert_eq!(log.lift(), 0);
        assert!(log.view(80).is_empty());
    }

    #[test]
    fn the_log_keeps_only_the_most_recent_lines() {
        let mut log = LogBuffer::new();
        for n in 0..=LOG_CAPACITY {
            log.push(&n.to_string(), Tone::App);
        }
        let rows = log.view(80);
        assert_eq!(rows.len(), LOG_CAPACITY);
        assert_eq!(rows[0].text, "1");
    }

    #[test]
    fn keys_map_to_commands_and_down_needs_a_second_press() {
        let plain = KeyModifiers::empty();
        assert_eq!(
            on_key(KeyCode::Char('r'), plain, false),
            Action::Command(DevCommand::Restart)
        );
        assert_eq!(
            on_key(KeyCode::Char('m'), plain, false),
            Action::Command(DevCommand::MigrateUp)
        );
        assert_eq!(on_key(KeyCode::Char('d'), plain, false), Action::AskConfirm);
        assert_eq!(
            on_key(KeyCode::Char('d'), plain, true),
            Action::Command(DevCommand::MigrateDown { steps: 1 })
        );
        assert_eq!(
            on_key(KeyCode::Char('x'), plain, true),
            Action::CancelConfirm
        );
        assert_eq!(on_key(KeyCode::Char('c'), plain, false), Action::ClearLog);
        assert_eq!(
            on_key(KeyCode::Char('c'), KeyModifiers::CONTROL, false),
            Action::Quit
        );
        assert_eq!(on_key(KeyCode::Char('q'), plain, false), Action::Quit);
        assert_eq!(
            on_key(KeyCode::PageUp, plain, false),
            Action::Scroll(PAGE as isize)
        );
        assert_eq!(on_key(KeyCode::End, plain, false), Action::ToBottom);
        assert_eq!(on_key(KeyCode::Char('z'), plain, false), Action::Nothing);
    }

    #[test]
    fn log_lines_are_coloured_by_where_they_come_from() {
        assert_eq!(
            tone_of(false, "2026-11-27T13:46:20.566  INFO listening"),
            Tone::App
        );
        assert_eq!(
            tone_of(false, "2026-11-27T13:46:20.566  WARN slow query"),
            Tone::Warning
        );
        assert_eq!(
            tone_of(false, "2026-11-27T13:46:20.566 ERROR boom"),
            Tone::Error
        );
        assert_eq!(
            tone_of(true, "thread 'main' panicked at src/main.rs:3:5:"),
            Tone::Error
        );
        assert_eq!(tone_of(true, "warning: unused variable"), Tone::Warning);
    }
}
