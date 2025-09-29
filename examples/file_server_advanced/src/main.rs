use silent::prelude::*;

fn ensure_static_assets(root: &str) {
    let dir = std::path::Path::new(root);
    if !dir.is_dir() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let index_path = dir.join("index.html");
    if !index_path.is_file() {
        std::fs::write(
            &index_path,
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Silent Advanced Static</title>
</head>
<body>
<h1>Silent 高级静态示例</h1>
<p>该页面由 example-file-server-advanced 自动生成。</p>
</body>
</html>"#,
        )
        .unwrap();
    }

    let doc_path = dir.join("docs");
    if !doc_path.is_dir() {
        std::fs::create_dir_all(&doc_path).unwrap();
    }
    let readme_path = doc_path.join("readme.txt");
    if !readme_path.is_file() {
        std::fs::write(&readme_path, "欢迎使用 Silent 高级静态资源示例！").unwrap();
    }
}

fn build_router(root: &str) -> Route {
    let options = StaticOptions::default()
        .with_directory_listing()
        .with_compression();

    Route::new("")
        .append(Route::new("health").get(|_req: Request| async move { Ok::<_, SilentError>("ok") }))
        .with_static_options(root, options)
}

fn main() {
    logger::fmt().init();
    let static_root = "static-advanced";
    ensure_static_assets(static_root);

    let route = build_router(static_root);
    Server::new().run(route);
}
