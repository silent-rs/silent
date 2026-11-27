//! 脚手架的模板文件，编译时嵌进工具。
//!
//! 占位符写成 `{{ name }}`，由 [`crate::new::render`] 替换。模板里只有占位符，没有条件和循环：
//! 需要按选项变化的内容（比如依赖的写法）在 Rust 里算好再传进来。

/// 一个模板文件。`path` 相对于应用的根目录。
pub(crate) struct Template {
    pub(crate) path: &'static str,
    pub(crate) body: &'static str,
}

/// 生成一个应用要写的全部文件。
pub(crate) fn app() -> Vec<Template> {
    let files: Vec<(&'static str, &'static str)> = vec![
        ("Cargo.toml", include_str!("../templates/app/Cargo.toml")),
        ("Makefile", include_str!("../templates/app/Makefile")),
        ("README.md", include_str!("../templates/app/README.md")),
        ("AGENTS.md", include_str!("../templates/app/AGENTS.md")),
        (
            "rustfmt.toml",
            include_str!("../templates/app/rustfmt.toml"),
        ),
        (".gitignore", include_str!("../templates/app/.gitignore")),
        (
            "silent-boot.toml",
            include_str!("../templates/app/silent-boot.toml"),
        ),
        (
            "config/app.toml",
            include_str!("../templates/app/config/app.toml"),
        ),
        (
            "migration/Cargo.toml",
            include_str!("../templates/app/migration/Cargo.toml"),
        ),
        (
            "migration/src/lib.rs",
            include_str!("../templates/app/migration/src/lib.rs"),
        ),
        (
            "migration/src/m20261126_000001_create_article.rs",
            include_str!("../templates/app/migration/src/m20261126_000001_create_article.rs"),
        ),
        (
            "entity/Cargo.toml",
            include_str!("../templates/app/entity/Cargo.toml"),
        ),
        (
            "entity/src/lib.rs",
            include_str!("../templates/app/entity/src/lib.rs"),
        ),
        (
            "entity/src/prelude.rs",
            include_str!("../templates/app/entity/src/prelude.rs"),
        ),
        (
            "entity/src/article.rs",
            include_str!("../templates/app/entity/src/article.rs"),
        ),
        ("src/lib.rs", include_str!("../templates/app/src/lib.rs")),
        ("src/main.rs", include_str!("../templates/app/src/main.rs")),
        ("src/app.rs", include_str!("../templates/app/src/app.rs")),
        (
            "src/api/mod.rs",
            include_str!("../templates/app/src/api/mod.rs"),
        ),
        (
            "src/api/article.rs",
            include_str!("../templates/app/src/api/article.rs"),
        ),
        (
            "src/dto/mod.rs",
            include_str!("../templates/app/src/dto/mod.rs"),
        ),
        (
            "src/dto/article.rs",
            include_str!("../templates/app/src/dto/article.rs"),
        ),
        (
            "src/services/mod.rs",
            include_str!("../templates/app/src/services/mod.rs"),
        ),
        (
            "src/services/article.rs",
            include_str!("../templates/app/src/services/article.rs"),
        ),
        (
            "web/dist/index.html",
            include_str!("../templates/app/web/dist/index.html"),
        ),
        (
            "tests/api.rs",
            include_str!("../templates/app/tests/api.rs"),
        ),
    ];
    files
        .into_iter()
        .map(|(path, body)| Template { path, body })
        .collect()
}
