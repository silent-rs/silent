//! M0 验证 V9：`dev` 运行循环的三个部分，对应 `docs/design.md` §16 的 V9。
//!
//! - 事件服务器：页面能连上，能收到 `reload` 和 `build-error`，能取到页面脚本。
//! - 监听器：源码变更会报信号，构建产物和文档不会。
//! - 运行循环：变更后重启应用并通知页面；停止时把应用一起停掉。
//!
//! macOS 的文件事件可能晚一秒左右才到，所以这里的时间上限放得比较宽。
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use silent_boot_cli::dev::{BoxFuture, Builder, DevLoop};
use silent_boot_cli::events::Events;
use silent_boot_cli::watch::SourceWatcher;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

/// 不构建，直接返回给定的可执行文件。
struct JustRun(PathBuf);

impl Builder for JustRun {
    fn build(&self) -> BoxFuture<'_, Result<PathBuf, String>> {
        Box::pin(async move { Ok(self.0.clone()) })
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
