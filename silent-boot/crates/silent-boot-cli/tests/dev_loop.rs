//! M0 验证 V9：`dev` 运行循环的三个部分，对应 `docs/design.md` §16 的 V9。
//!
//! - 事件服务器：页面能连上，能收到 `reload` 和 `build-error`，能取到页面脚本。
//! - 监听器：源码变更会报信号，构建产物和文档不会。
//! - 运行循环：变更后重启应用并通知页面；停止时把应用一起停掉。
//! - 构建没过时不停旧进程，上一次的版本接着服务；构建成功之后才换进程（文末的测试）。
//!
//! macOS 的文件事件可能晚一秒左右才到，所以这里的时间上限放得比较宽。
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use silent_boot_cli::dev::{BoxFuture, Builder, DevCommand, DevLoop, DevStatus, Reporter};
use silent_boot_cli::events::Events;
use silent_boot_cli::watch::SourceWatcher;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

/// 不构建，直接返回给定的可执行文件。
struct JustRun(PathBuf);

impl Builder for JustRun {
    fn build(&self) -> BoxFuture<'_, Result<PathBuf, String>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }

    fn bin_path(&self) -> PathBuf {
        self.0.clone()
    }
}

/// 按开关决定这次构建成不成功，并记下“构建开始的时候，上一个应用还活着吗”。
struct CheckThenBuild {
    script: PathBuf,
    pid_file: PathBuf,
    fail: Arc<AtomicBool>,
    previous_alive: Arc<Mutex<Vec<bool>>>,
}

impl Builder for CheckThenBuild {
    fn bin_path(&self) -> PathBuf {
        self.script.clone()
    }

    fn build(&self) -> BoxFuture<'_, Result<PathBuf, String>> {
        Box::pin(async move {
            // 第一次启动时还没有应用；之后每次重启构建都先于停旧进程，这里读到的应该是活着的上一个。
            if let Ok(pid) = std::fs::read_to_string(&self.pid_file) {
                self.previous_alive
                    .lock()
                    .expect("previous_alive")
                    .push(process_alive(pid.trim()));
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err("error[E0425]: cannot find value `oops` in this scope".to_owned());
            }
            Ok(self.script.clone())
        })
    }
}

/// 一个只会发请求、按标记读响应的小客户端，记得已经读到哪里。
struct Client {
    stream: TcpStream,
    text: String,
    cursor: usize,
}

impl Client {
    async fn get(addr: std::net::SocketAddr, path: &str) -> Self {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("send");
        Self {
            stream,
            text: String::new(),
            cursor: 0,
        }
    }

    /// 一直读到 `marker` 出现，返回从头到标记为止的内容。
    async fn expect(&mut self, marker: &str, within: Duration) -> String {
        let deadline = Instant::now() + within;
        loop {
            if let Some(at) = self.text[self.cursor..].find(marker) {
                let end = self.cursor + at + marker.len();
                let found = self.text[self.cursor..end].to_owned();
                self.cursor = end;
                return found;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            let mut buf = [0u8; 4096];
            let read = tokio::time::timeout(left, self.stream.read(&mut buf))
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {marker:?}"));
            let n = read.expect("read");
            assert!(n > 0, "connection closed while waiting for {marker:?}");
            self.text.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    }

    async fn read_all(&mut self) -> String {
        let mut rest = Vec::new();
        self.stream.read_to_end(&mut rest).await.expect("read");
        format!("{}{}", self.text, String::from_utf8_lossy(&rest))
    }
}

fn process_alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 轮询到文件里出现一个 PID 为止。
async fn wait_for_pid(file: &Path, previous: Option<&str>) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(text) = std::fs::read_to_string(file) {
            let pid = text.trim().to_owned();
            if !pid.is_empty() && Some(pid.as_str()) != previous {
                return pid;
            }
        }
        assert!(
            Instant::now() < deadline,
            "no new pid in {}",
            file.display()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 等到第 `n` 次重启的构建开始过。
async fn wait_for_rebuilds(previous_alive: &Mutex<Vec<bool>>, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while previous_alive.lock().expect("previous_alive").len() < n {
        assert!(Instant::now() < deadline, "the loop did not rebuild");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn v9_events_server_streams_events_and_serves_the_page_script() {
    let events = Events::bind("127.0.0.1:0".parse().unwrap())
        .await
        .expect("bind");
    let addr = events.addr();

    let mut page = Client::get(addr, "/__dev__/events").await;
    let headers = page.expect("\r\n\r\n", Duration::from_secs(5)).await;
    assert!(headers.contains("200 OK"), "{headers}");
    assert!(headers.contains("text/event-stream"), "{headers}");
    let first = page.expect("retry: 1000", Duration::from_secs(5)).await;
    assert!(
        first.starts_with(':'),
        "the first frame is a comment: {first}"
    );

    events.publish("reload", "42");
    let frame = page.expect("data: 42\n\n", Duration::from_secs(5)).await;
    assert!(frame.contains("event: reload\n"), "{frame}");

    // 多行的构建错误：每一行前面都是 data:，浏览器收到时用换行拼回去。
    events.publish("build-error", "error[E0425]\n  --> src/main.rs:3:9");
    let frame = page
        .expect("data:   --> src/main.rs:3:9\n\n", Duration::from_secs(5))
        .await;
    assert!(
        frame.contains("event: build-error\ndata: error[E0425]\ndata:   --> src/main.rs:3:9"),
        "{frame}"
    );

    let mut script = Client::get(addr, "/__dev__/reload.js").await;
    let script = script.read_all().await;
    assert!(script.contains("200 OK"), "{script}");
    assert!(
        script.contains(&format!("http://{addr}/__dev__/events")),
        "{script}"
    );
    assert!(script.contains("location.reload()"), "{script}");
    assert!(script.contains("build-error"), "{script}");

    let mut missing = Client::get(addr, "/").await;
    let missing = missing.read_all().await;
    assert!(missing.contains("404 Not Found"), "{missing}");
}

#[tokio::test]
async fn v9_watcher_reports_source_changes_and_ignores_build_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::create_dir_all(dir.path().join("target/debug")).expect("target");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("main.rs");

    let mut watcher = SourceWatcher::start(&[dir.path().to_path_buf()], &[]).expect("watcher");
    let changes = watcher.changes();

    std::fs::write(
        src.join("main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .expect("edit");
    let first = tokio::time::timeout(Duration::from_secs(10), changes.recv()).await;
    assert!(
        matches!(first, Ok(Some(()))),
        "a source change must be reported"
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    while changes.try_recv().is_ok() {}

    std::fs::write(dir.path().join("target/debug/app.o"), b"object").expect("build output");
    std::fs::write(dir.path().join("notes.md"), "docs").expect("docs");
    let unexpected = tokio::time::timeout(Duration::from_millis(1500), changes.recv()).await;
    assert!(unexpected.is_err(), "build output and docs must be ignored");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v9_dev_loop_restarts_the_app_on_change_and_stops_it_on_shutdown() {
    let dir = tempfile::tempdir().expect("temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("main.rs");
    // “应用”：把自己的 PID 写进 pid.txt，然后一直睡。
    let script = dir.path().join("app.sh");
    std::fs::write(&script, "#!/bin/sh\necho $$ > pid.txt\nexec sleep 60\n").expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let pid_file = dir.path().join("pid.txt");

    let events = Arc::new(
        Events::bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("bind"),
    );
    let mut page = Client::get(events.addr(), "/__dev__/events").await;
    page.expect("retry: 1000", Duration::from_secs(5)).await;

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let task = tokio::spawn({
        let events = events.clone();
        let cwd = dir.path().to_path_buf();
        async move {
            let mut watcher = SourceWatcher::start(&[cwd.join("src")], &[]).expect("watcher");
            DevLoop::new(JustRun(script), cwd, events)
                .debounce(Duration::from_millis(100))
                .grace(Duration::from_secs(2))
                .run(watcher.changes(), async {
                    let _ = stop_rx.await;
                })
                .await;
        }
    });

    let first = wait_for_pid(&pid_file, None).await;
    assert!(process_alive(&first), "the app should be running");

    // 改一个源码文件：应用被重启，页面收到 reload。
    let changed_at = Instant::now();
    std::fs::write(src.join("main.rs"), "fn main() { println!(\"v2\"); }\n").expect("edit");
    let second = wait_for_pid(&pid_file, Some(&first)).await;
    let restarted_after = changed_at.elapsed();
    let frame = page
        .expect("event: reload\ndata: ", Duration::from_secs(5))
        .await;
    println!(
        "从保存到应用重启 {} 毫秒，到页面收到通知 {} 毫秒",
        restarted_after.as_millis(),
        changed_at.elapsed().as_millis()
    );
    assert!(frame.contains("event: reload\n"), "{frame}");
    assert_ne!(first, second, "the app must have been restarted");
    assert!(process_alive(&second), "the new app should be running");
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(&first) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!process_alive(&first), "the old app must have been stopped");

    // 停止循环：应用跟着停掉。
    stop_tx.send(()).expect("stop the loop");
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the loop should finish")
        .expect("loop task");
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(&second) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !process_alive(&second),
        "stopping the loop must stop the app"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_build_keeps_the_previous_app_running_until_a_build_succeeds() {
    let dir = tempfile::tempdir().expect("temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("main.rs");
    let script = dir.path().join("app.sh");
    std::fs::write(&script, "#!/bin/sh\necho $$ > pid.txt\nexec sleep 60\n").expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let pid_file = dir.path().join("pid.txt");
    let fail = Arc::new(AtomicBool::new(false));
    let previous_alive = Arc::new(Mutex::new(Vec::new()));

    let events = Arc::new(
        Events::bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("bind"),
    );
    let mut page = Client::get(events.addr(), "/__dev__/events").await;
    page.expect("retry: 1000", Duration::from_secs(5)).await;

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let task = tokio::spawn({
        let events = events.clone();
        let cwd = dir.path().to_path_buf();
        let builder = CheckThenBuild {
            script,
            pid_file: pid_file.clone(),
            fail: fail.clone(),
            previous_alive: previous_alive.clone(),
        };
        async move {
            let mut watcher = SourceWatcher::start(&[cwd.join("src")], &[]).expect("watcher");
            DevLoop::new(builder, cwd, events)
                .debounce(Duration::from_millis(100))
                .grace(Duration::from_secs(2))
                .run(watcher.changes(), async {
                    let _ = stop_rx.await;
                })
                .await;
        }
    });
    let first = wait_for_pid(&pid_file, None).await;
    assert!(process_alive(&first), "the app should be running");

    // 构建没过：旧应用继续跑，页面收到 build-error，里面是编译错误。
    fail.store(true, Ordering::SeqCst);
    std::fs::write(src.join("main.rs"), "fn main() { println!(\"v2\"); }\n").expect("edit");
    wait_for_rebuilds(&previous_alive, 1).await;
    let frame = page
        .expect("cannot find value `oops`", Duration::from_secs(5))
        .await;
    assert!(frame.contains("event: build-error"), "{frame}");
    assert!(frame.contains("上一次构建成功的版本"), "{frame}");
    // 留出足够的时间：循环要是真的去停旧进程了，这里能看出来。
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        process_alive(&first),
        "a failed build must leave the running app alone"
    );
    assert_eq!(
        std::fs::read_to_string(&pid_file).expect("pid").trim(),
        first,
        "no new app may have been started"
    );

    // 构建过了：构建期间旧应用还在跑，构建完才换进程，页面收到 reload。
    fail.store(false, Ordering::SeqCst);
    std::fs::write(src.join("main.rs"), "fn main() { println!(\"v3\"); }\n").expect("edit");
    let second = wait_for_pid(&pid_file, Some(&first)).await;
    wait_for_rebuilds(&previous_alive, 2).await;
    assert_eq!(
        *previous_alive.lock().expect("previous_alive"),
        vec![true, true],
        "the previous app must be alive while the new one is being built"
    );
    let frame = page
        .expect("event: reload\ndata: ", Duration::from_secs(5))
        .await;
    assert!(frame.contains("event: reload\n"), "{frame}");
    assert_ne!(first, second, "the app must have been restarted");
    assert!(process_alive(&second), "the new app should be running");
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(&first) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !process_alive(&first),
        "the old app must be stopped once the new one is built"
    );

    stop_tx.send(()).expect("stop the loop");
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the loop should finish")
        .expect("loop task");
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(&second) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !process_alive(&second),
        "stopping the loop must stop the app"
    );
}

/// 记下循环说过的话和应用输出过的东西，测试好断言。
#[derive(Clone, Default)]
struct Notes(Arc<Mutex<Vec<String>>>);

impl Reporter for Notes {
    fn note(&self, line: String) {
        self.0.lock().expect("notes").push(line);
    }

    fn output(&self, _stderr: bool, line: String) {
        self.0.lock().expect("notes").push(line);
    }

    fn status(&self, _status: DevStatus) {}
}

/// 等到循环说过含 `needle` 的话。
async fn wait_for_said(notes: &Mutex<Vec<String>>, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !notes
        .lock()
        .expect("notes")
        .iter()
        .any(|line| line.contains(needle))
    {
        assert!(
            Instant::now() < deadline,
            "{needle:?} never showed up in {:?}",
            notes.lock().expect("notes")
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 等到 `log` 里至少有 `lines` 行。
async fn wait_for_log(log: &Path, lines: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while std::fs::read_to_string(log)
        .map(|text| text.lines().count())
        .unwrap_or(0)
        < lines
    {
        assert!(
            Instant::now() < deadline,
            "{} never reached {lines} lines",
            log.display()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

const SCRIPT_URL: &str = "http://127.0.0.1:9230/__dev__/reload.js";

/// “应用”：认 `migrate` 的三个子命令，其余情况当作启动。子命令的输出同时写进文件和标准输出。
fn app_with_migrate(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let script = dir.join("app.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
case "$1 $2" in
  "migrate up") echo "up" | tee -a log.txt; exit 0 ;;
  "migrate down") echo "down $4" | tee -a log.txt; exit 0 ;;
  "migrate status") echo status >> log.txt; echo '[]'; exit 0 ;;
esac
echo "run $SB_DEV_RELOAD_URL" >> log.txt
echo $$ > pid.txt
exec sleep 60
"#,
    )
    .expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    (script, dir.join("log.txt"), dir.join("pid.txt"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn commands_restart_the_app_and_run_the_migrate_subcommands_of_the_app() {
    let dir = tempfile::tempdir().expect("temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("main.rs");
    let (script, log, pid_file) = app_with_migrate(dir.path());
    let notes = Notes::default();

    let events = Arc::new(
        Events::bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("bind"),
    );
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let (commands_tx, commands_rx) = mpsc::channel::<DevCommand>(16);
    let task = tokio::spawn({
        let events = events.clone();
        let cwd = dir.path().to_path_buf();
        let reporter: Arc<dyn Reporter> = Arc::new(notes.clone());
        async move {
            let mut watcher = SourceWatcher::start(&[cwd.join("src")], &[]).expect("watcher");
            DevLoop::with_reporter(JustRun(script), cwd, events, reporter)
                .envs(vec![(
                    "SB_DEV_RELOAD_URL".to_owned(),
                    SCRIPT_URL.to_owned(),
                )])
                .debounce(Duration::from_millis(100))
                .grace(Duration::from_secs(2))
                .migrate_commands(true)
                .run_with_commands(watcher.changes(), commands_rx, async {
                    let _ = stop_rx.await;
                })
                .await;
        }
    });
    let first = wait_for_pid(&pid_file, None).await;
    assert!(process_alive(&first), "the app should be running");
    // 启动之前数过一次待执行的迁移。
    wait_for_log(&log, 2).await;

    commands_tx
        .send(DevCommand::MigrateUp)
        .await
        .expect("send MigrateUp");
    wait_for_said(&notes.0, "`migrate up` 执行完").await;
    commands_tx
        .send(DevCommand::MigrateDown { steps: 1 })
        .await
        .expect("send MigrateDown");
    wait_for_said(&notes.0, "down 1").await;
    commands_tx
        .send(DevCommand::MigrateStatus)
        .await
        .expect("send MigrateStatus");
    wait_for_said(&notes.0, "`migrate status` 执行完").await;
    // 应用没有因为迁移被停掉。
    assert!(process_alive(&first));
    // 启动之前数一次待执行的迁移；每条迁移命令之后（包括看状态）再数一次。
    assert_eq!(
        std::fs::read_to_string(&log)
            .expect("log")
            .lines()
            .collect::<Vec<_>>(),
        vec![
            "status",
            "run http://127.0.0.1:9230/__dev__/reload.js",
            "up",
            "status",
            "down 1",
            "status",
            "status",
            "status",
        ]
    );

    commands_tx
        .send(DevCommand::Restart)
        .await
        .expect("send Restart");
    wait_for_said(&notes.0, "手动重启").await;
    let second = wait_for_pid(&pid_file, Some(&first)).await;
    assert_ne!(first, second, "the app must have been restarted");
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(&first) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!process_alive(&first), "the old app must have been stopped");

    stop_tx.send(()).expect("stop the loop");
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the loop should finish")
        .expect("loop task");
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(&second) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !process_alive(&second),
        "stopping the loop must stop the app"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn outside_a_silent_boot_app_migration_commands_explain_and_do_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("main.rs");
    let (script, log, pid_file) = app_with_migrate(dir.path());
    let notes = Notes::default();

    let events = Arc::new(
        Events::bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("bind"),
    );
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let (commands_tx, commands_rx) = mpsc::channel::<DevCommand>(16);
    let task = tokio::spawn({
        let events = events.clone();
        let cwd = dir.path().to_path_buf();
        let reporter: Arc<dyn Reporter> = Arc::new(notes.clone());
        async move {
            std::fs::create_dir_all(cwd.join("idle")).expect("idle dir");
            let mut watcher = SourceWatcher::start(&[cwd.join("idle")], &[]).expect("watcher");
            DevLoop::with_reporter(JustRun(script), cwd, events, reporter)
                .envs(vec![(
                    "SB_DEV_RELOAD_URL".to_owned(),
                    SCRIPT_URL.to_owned(),
                )])
                .migrate_commands(false)
                .run_with_commands(watcher.changes(), commands_rx, async {
                    let _ = stop_rx.await;
                })
                .await;
        }
    });
    let first = wait_for_pid(&pid_file, None).await;
    wait_for_log(&log, 1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // 这里不是 silent-boot 应用：启动时不去数待执行的迁移。
    assert_eq!(
        std::fs::read_to_string(&log).expect("log").lines().count(),
        1
    );

    commands_tx
        .send(DevCommand::MigrateUp)
        .await
        .expect("send MigrateUp");
    wait_for_said(&notes.0, "没有 `migrate up` 子命令").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        std::fs::read_to_string(&log).expect("log").lines().count(),
        1
    );
    assert!(process_alive(&first));

    stop_tx.send(()).expect("stop the loop");
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the loop should finish")
        .expect("loop task");
}
