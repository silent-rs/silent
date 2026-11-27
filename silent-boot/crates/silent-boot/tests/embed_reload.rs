//! M0 验证：两个"不重启就能生效"的前提，对应 `docs/design.md` §16 的 V7、V8。
//!
//! - V7：rust-embed 在 debug 构建下每次都从磁盘读，前端改动不用重新编译后端。
//! - V8：Tera 启动时只加载一次，调用 `full_reload()` 就能读到新增和修改的模板。
use std::fs;

#[derive(rust_embed::RustEmbed)]
#[folder = "tests/common/web/"]
struct Assets;

#[test]
fn v7_rust_embed_reads_from_disk_in_debug_builds() {
    let index = Assets::get("index.html").expect("index.html is embedded");
    assert_eq!(index.data.as_ref(), b"<h1>app</h1>\n");
    assert!(Assets::get("missing.js").is_none());

    // debug 构建不缓存：刚写进磁盘的文件立刻能读到。
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common/web");
    let hot = dir.join("hot-v7.txt");
    fs::write(&hot, "fresh").expect("write");
    let fresh = Assets::get("hot-v7.txt").expect("freshly written file is visible");
    fs::remove_file(&hot).expect("cleanup");
    assert_eq!(fresh.data.as_ref(), b"fresh");
}

#[test]
fn v8_tera_full_reload_picks_up_changed_and_added_templates()
-> Result<(), Box<dyn std::error::Error>> {
    use tera::{Context, Tera};

    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("a.html"), "old")?;
    let mut tera = Tera::new();
    tera.load_from_glob(&format!("{}/*.html", dir.path().display()))?;
    assert_eq!(tera.render("a.html", &Context::new())?, "old");

    fs::write(dir.path().join("a.html"), "new")?;
    fs::write(dir.path().join("b.html"), "added")?;
    // Tera 不会自己感知磁盘变化：不重载时读到的还是旧内容，新文件不存在。
    assert_eq!(tera.render("a.html", &Context::new())?, "old");
    assert!(!tera.get_template_names().any(|name| name == "b.html"));

    tera.full_reload()?;
    assert_eq!(tera.render("a.html", &Context::new())?, "new");
    assert_eq!(tera.render("b.html", &Context::new())?, "added");
    Ok(())
}
