//! M1 验证：`generate module` 的第一步。
//!
//! - 从 `article` 表生成 dto、service、api，并登记到三个 `mod.rs`；
//! - 已经存在的文件不覆盖；
//! - 生成出来的项目能编译、`clippy -D warnings` 干净，示例应用自带的接口测试照常通过：
//!   也就是生成的模块和手写的示例模块行为一致。
//!
//! 做法：把 `examples/demo` 拷到 `examples/` 下的一个临时目录（相对路径的依赖在那里照样解析得到），
//! 删掉手写的示例模块，用生成器补回来，再对这份拷贝跑 cargo。
#![cfg(feature = "codegen")]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::app::AppMigrator;
use silent_boot::cli::module::{self, ModuleOptions};
use silent_boot::db::migrate::{FrameworkMigrator, MigrationRunner};

/// 拷贝出来的示例应用。离开作用域时删除。
struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let source = examples.join("demo");
        let path = examples.join(".module-check");
        let _ = fs::remove_dir_all(&path);
        copy_dir(&source, &path);
        // 去掉手写的示例模块，`mod.rs` 换成只带着受管区块的版本。
        for file in [
            "src/api/article.rs",
            "src/dto/article.rs",
            "src/services/article.rs",
        ] {
            fs::remove_file(path.join(file)).expect("remove the hand-written module");
        }
        for (file, bare) in [
            ("src/api/mod.rs", BARE_API_MOD),
            ("src/dto/mod.rs", BARE_DTO_MOD),
            ("src/services/mod.rs", BARE_DTO_MOD),
        ] {
            fs::write(path.join(file), bare).expect("bare mod.rs");
        }
        // 生成器写的 404 说明是表名，不是手写模块里的中文。
        let tests = path.join("tests/api.rs");
        let text = fs::read_to_string(&tests).expect("tests/api.rs");
        fs::write(&tests, text.replace("文章不存在", "article不存在")).expect("tests/api.rs");
        Self(path)
    }

    fn cargo(&self, args: &[&str]) -> Output {
        Command::new("cargo")
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run cargo")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 只带着受管区块的 `mod.rs`：生成器往区块里登记。
const BARE_API_MOD: &str = "use silent_boot::prelude::*;\n\n// <silent-boot:gen:mods>\n// </silent-boot:gen:mods>\n\npub fn routes() -> Route {\n    Route::new(\"api\")\n        // <silent-boot:gen:routes>\n        // </silent-boot:gen:routes>\n}\n";
const BARE_DTO_MOD: &str = "// <silent-boot:gen:mods>\n// </silent-boot:gen:mods>\n";

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        // 构建产物和本地数据不拷。锁文件有就带上：省得每次重新解析依赖。
        if matches!(name.to_str(), Some("target" | "data")) {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        if source.is_dir() {
            copy_dir(&source, &target);
        } else {
            fs::copy(&source, &target).expect("copy file");
        }
    }
}

#[tokio::test]
async fn v36_generate_module_rebuilds_the_sample_module_from_its_table() {
    let (dir, db) = common::temp_db().await;
    MigrationRunner::new("v36")
        .up(&db, &FrameworkMigrator, &AppMigrator, None)
        .await
        .expect("create the sample tables");
    let url = common::url(&dir);
    let sandbox = Sandbox::new();
    let options = ModuleOptions {
        table: "article".to_owned(),
        ..ModuleOptions::default()
    };

    let generated = module::generate_module(&url, &sandbox.0, &options)
        .await
        .expect("generate the module");
    let mut names = generated
        .files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "src/api/article.rs",
            "src/api/mod.rs",
            "src/dto/article.rs",
            "src/dto/mod.rs",
            "src/services/article.rs",
            "src/services/mod.rs",
        ],
    );
    assert!(generated.skipped.is_empty());
    let api_mod = fs::read_to_string(sandbox.0.join("src/api/mod.rs")).expect("api/mod.rs");
    assert!(api_mod.contains("pub mod article;"), "{api_mod}");
    assert!(api_mod.contains(".append(article::route())"), "{api_mod}");
    let dto = fs::read_to_string(sandbox.0.join("src/dto/article.rs")).expect("dto");
    assert!(dto.contains("pub struct CreateArticleDto"), "{dto}");
    // `title` 不允许为空、没有默认值：必填且不能是空串。
    assert!(
        dto.contains("#[validate(length(min = 1))]\n    pub title: String,"),
        "{dto}"
    );
    assert!(dto.contains("pub status: Option<i8>"), "{dto}");
    let service = fs::read_to_string(sandbox.0.join("src/services/article.rs")).expect("service");
    assert!(service.contains("Column::DeletedAt.is_null()"), "{service}");
    assert!(service.contains("Set(now)"), "{service}");

    // 再生成一次：三个模块文件都在，不动；登记也没变。
    let again = module::generate_module(&url, &sandbox.0, &options)
        .await
        .expect("generate again");
    assert_eq!(again.files.len(), 0, "{again:?}");
    assert_eq!(again.skipped.len(), 3, "{again:?}");

    // 这份拷贝要能编译、lint 干净，示例自带的接口测试照常通过。
    let clippy = sandbox.cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]);
    assert!(
        clippy.status.success(),
        "clippy on the generated module failed:\n{}\n{}",
        String::from_utf8_lossy(&clippy.stdout),
        String::from_utf8_lossy(&clippy.stderr)
    );
    let test = sandbox.cargo(&["nextest", "run"]);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&test.stdout),
        String::from_utf8_lossy(&test.stderr),
    );
    if !test.status.success() && stderr.contains("no such subcommand")
        || stdout.contains("no such subcommand")
    {
        let test = sandbox.cargo(&["test", "--quiet"]);
        assert!(
            test.status.success(),
            "the example's tests failed against the generated module:\n{}\n{}",
            String::from_utf8_lossy(&test.stdout),
            String::from_utf8_lossy(&test.stderr)
        );
    } else {
        assert!(test.status.success(), "{stdout}\n{stderr}");
        let all = format!("{stdout}\n{stderr}");
        assert!(all.contains("4 passed"), "{all}");
    }
}

#[tokio::test]
async fn v36_generate_module_reports_missing_tables_and_unsuitable_ones() {
    let (dir, db) = common::temp_db().await;
    MigrationRunner::new("v36b")
        .up(&db, &FrameworkMigrator, &AppMigrator, None)
        .await
        .expect("create the sample tables");
    let url = common::url(&dir);
    let app = tempfile::tempdir().expect("temp dir");
    let options = ModuleOptions {
        table: "nope".to_owned(),
        ..ModuleOptions::default()
    };
    let err = module::generate_module(&url, app.path(), &options)
        .await
        .expect_err("no such table");
    let text = err.to_string();
    assert!(text.contains("nope") && text.contains("article"), "{text}");

    // 应用里没有 `mod.rs`：说清楚缺哪个文件。
    let options = ModuleOptions {
        table: "article".to_owned(),
        ..ModuleOptions::default()
    };
    let err = module::generate_module(&url, app.path(), &options)
        .await
        .expect_err("missing mod.rs");
    assert!(
        err.to_string().contains("src/dto/mod.rs"),
        "{}",
        err.to_string()
    );
    // `mod.rs` 里没有受管区块：说清楚是哪个文件、哪个标记。
    fs::create_dir_all(app.path().join("src/api")).expect("src/api");
    fs::create_dir_all(app.path().join("src/dto")).expect("src/dto");
    fs::create_dir_all(app.path().join("src/services")).expect("src/services");
    fs::write(app.path().join("src/dto/mod.rs"), BARE_DTO_MOD).expect("dto/mod.rs");
    fs::write(app.path().join("src/services/mod.rs"), BARE_DTO_MOD).expect("services/mod.rs");
    fs::write(app.path().join("src/api/mod.rs"), "pub fn routes() {}\n").expect("api/mod.rs");
    let err = module::generate_module(&url, app.path(), &options)
        .await
        .expect_err("missing markers");
    let text = err.to_string();
    assert!(text.contains("src/api/mod.rs"), "{text}");
    assert!(text.contains("<silent-boot:gen:mods>"), "{text}");
    // 失败时不留下任何文件。
    assert!(!app.path().join("src/api/article.rs").exists());
}
