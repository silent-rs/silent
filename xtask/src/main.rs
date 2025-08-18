use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs, io};

#[derive(Clone, Debug, ValueEnum)]
enum Scenario {
    A,
    B,
    C,
}

#[derive(Clone, Debug, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Csv,
}

#[derive(Parser, Debug)]
#[command(name = "xtask", version, about = "Silent workspace helper tasks")]
struct XtaskCli {
    #[command(subcommand)]
    cmd: XtaskCmd,
}

#[derive(Subcommand, Debug)]
enum XtaskCmd {
    /// Run local benchmark service and optional bombardier load
    Bench {
        /// Scenario to run: A|B|C
        #[arg(short = 's', long = "scenario", value_enum, default_value_t = Scenario::A)]
        scenario: Scenario,
        /// Port to bind (default 8080)
        #[arg(short = 'p', long = "port", default_value_t = 8080)]
        port: u16,
        /// bombardier duration, e.g. 30s
        #[arg(short = 'd', long = "duration", default_value = "30s")]
        duration: String,
        /// bombardier concurrency
        #[arg(short = 'c', long = "concurrency", default_value_t = 256)]
        concurrency: u32,
        /// Format bombardier result: json|csv (if omitted, stream human output)
        #[arg(short = 'f', long = "format", value_enum)]
        format: Option<OutputFormat>,
        /// Output file path for formatted result (if omitted, print to stdout)
        #[arg(short = 'o', long = "out-file")]
        out_file: Option<PathBuf>,
        /// When format=json, prune bombardier output to key fields (rps_avg, p50, p90, p99)
        #[arg(long = "prune", default_value_t = false)]
        prune: bool,
        /// Only run server (do not invoke bombardier)
        #[arg(long = "run-only", default_value_t = false)]
        run_only: bool,
    },
}

fn main() -> io::Result<()> {
    let cli = XtaskCli::parse();
    match cli.cmd {
        XtaskCmd::Bench {
            scenario,
            port,
            duration,
            concurrency,
            format,
            out_file,
            prune,
            run_only,
        } => {
            run_bench(
                scenario,
                port,
                &duration,
                concurrency,
                format,
                out_file,
                prune,
                run_only,
            )?;
        }
    }
    Ok(())
}

fn run_bench(
    s: Scenario,
    port: u16,
    duration: &str,
    concurrency: u32,
    format: Option<OutputFormat>,
    out_file: Option<PathBuf>,
    prune: bool,
    run_only: bool,
) -> io::Result<()> {
    let scenario = match s {
        Scenario::A => "A",
        Scenario::B => "B",
        Scenario::C => "C",
    };

    // Spawn benchmark server: SCENARIO=... PORT=...
    let mut server_cmd = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    server_cmd
        .arg("run")
        .arg("-p")
        .arg("benchmark")
        .arg("--release")
        .env("SCENARIO", scenario)
        .env("PORT", port.to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    println!(
        "[xtask] launching server: SCENARIO={} PORT={} => cargo run -p benchmark --release",
        scenario, port
    );

    let mut child = server_cmd.spawn()?;

    // If only run server, wait and return
    if run_only {
        let status = child.wait()?;
        println!("[xtask] server exited with status: {}", status);
        return Ok(());
    }

    // Give the server a moment to boot
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Prepare bombardier target URL
    let target = match scenario {
        "A" => format!("http://127.0.0.1:{}/", port),
        "B" => format!(
            "http://127.0.0.1:{}/b/abc/123/xyz?q1=a&q2=b&q3=42&q4=true&q5=z",
            port
        ),
        "C" => format!("http://127.0.0.1:{}/static", port),
        _ => unreachable!(),
    };

    // If scenario C, fetch ETag for 304 path
    let mut extra_header: Option<String> = None;
    if scenario == "C" {
        // 检查 curl 是否存在
        let curl_ok = which::which("curl").is_ok();
        if curl_ok {
            if let Ok(output) = Command::new("curl").arg("-sI").arg(&target).output() {
                if output.status.success() {
                    if let Ok(head) = String::from_utf8(output.clone().stdout) {
                        for line in head.lines() {
                            if let Some(v) = line.strip_prefix("etag: ") {
                                let v = v.trim();
                                extra_header = Some(format!("If-None-Match: {}", v));
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            eprintln!("[xtask] curl 未安装，跳过场景 C 的 If-None-Match 头设置");
        }
    }

    // Preflight: ensure bombardier is available
    if which::which("bombardier").is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "未找到 bombardier 可执行文件，请先安装：brew install bombardier 或参考 https://github.com/codesenberg/bombardier",
        ));
    }

    // Run bombardier
    let mut bombardier = Command::new("bombardier");
    bombardier
        .arg("-c")
        .arg(concurrency.to_string())
        .arg("-d")
        .arg(duration)
        .stderr(Stdio::inherit());
    if let Some(h) = extra_header.as_ref() {
        bombardier.arg("-H").arg(h);
    }
    if let Some(fmt) = &format {
        bombardier.arg("-o").arg(match fmt {
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
        });
        bombardier.stdout(Stdio::piped());
        // 屏蔽 bombardier 进度条（stderr）
        bombardier.stderr(Stdio::null());
    } else {
        bombardier.stdout(Stdio::inherit());
        bombardier.stderr(Stdio::inherit());
    }
    bombardier.arg(&target);

    // 若直接输出到 stdout（format 且无 out_file），避免打印多余日志影响 JSON 纯净性
    if !(format.is_some() && out_file.is_none()) {
        println!(
            "[xtask] running bombardier on {} (c={}, d={}, fmt={:?})",
            target, concurrency, duration, format
        );
    }

    // 执行 bombardier
    if format.is_some() {
        let output = bombardier.output()?;
        let body = String::from_utf8_lossy(&output.stdout).to_string();

        let content = if matches!(format, Some(OutputFormat::Json)) && prune {
            match prune_bombardier_json(&body, &scenario, port, concurrency, duration) {
                Ok(pruned) => pruned,
                Err(e) => {
                    eprintln!("[xtask] prune failed: {}. Fallback to raw.", e);
                    body
                }
            }
        } else {
            body
        }
        .split("\n")
        .last()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

        if let Some(ref path) = out_file {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).ok();
            }
            std::fs::write(&path, content.as_bytes())?;
            println!("[xtask] written formatted result to {}", path.display());
        } else {
            println!("{}", content);
        }
        // 若输出到 stdout，仅输出结果数据，不追加状态
        if out_file.is_some() {
            println!("[xtask] bombardier exit status: {}", output.status);
        }
    } else {
        let status = bombardier.status()?;
        println!("[xtask] bombardier exit status: {}", status);
    }

    // Terminate server process
    let _ = child.kill();
    let _ = child.wait();

    Ok(())
}

fn prune_bombardier_json(
    raw: &str,
    scenario: &str,
    port: u16,
    concurrency: u32,
    duration: &str,
) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;

    let result = v.get("result").unwrap_or(&serde_json::Value::Null);

    let rps_avg = result
        .get("rps")
        .and_then(|x| x.get("avg"))
        .or_else(|| v.get("rps").and_then(|x| x.get("avg")))
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);

    let latency = result.get("latency").unwrap_or(&serde_json::Value::Null);
    let mut p50 = latency.get("p50").and_then(|x| x.as_f64());
    let mut p90 = latency.get("p90").and_then(|x| x.as_f64());
    let mut p99 = latency.get("p99").and_then(|x| x.as_f64());
    if p50.is_none() || p90.is_none() || p99.is_none() {
        if let Some(percentiles) = latency.get("percentiles") {
            p50 = p50.or_else(|| percentiles.get("50").and_then(|x| x.as_f64()));
            p90 = p90.or_else(|| percentiles.get("90").and_then(|x| x.as_f64()));
            p99 = p99.or_else(|| percentiles.get("99").and_then(|x| x.as_f64()));
            p50 = p50.or_else(|| percentiles.get("50.0").and_then(|x| x.as_f64()));
            p90 = p90.or_else(|| percentiles.get("90.0").and_then(|x| x.as_f64()));
            p99 = p99.or_else(|| percentiles.get("99.0").and_then(|x| x.as_f64()));
        }
    }

    // 环境信息
    let ts = chrono::Local::now().naive_local().to_string();
    let hostname = get_hostname();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let obj = serde_json::json!({
        "scenario": scenario,
        "port": port,
        "concurrency": concurrency,
        "duration": duration,
        "timestamp_local": ts,
        "hostname": hostname,
        "os": os,
        "arch": arch,
        "rps_avg": rps_avg,
        "p50_ms": p50.unwrap_or(0.0),
        "p90_ms": p90.unwrap_or(0.0),
        "p99_ms": p99.unwrap_or(0.0),
    });
    serde_json::to_string_pretty(&obj).map_err(|e| e.to_string())
}

fn get_hostname() -> String {
    if let Ok(h) = std::env::var("HOSTNAME") {
        return h;
    }
    if let Ok(output) = std::process::Command::new("hostname").output() {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                return s.trim().to_string();
            }
        }
    }
    String::from("unknown-host")
}
