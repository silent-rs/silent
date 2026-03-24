fn main() {
    #[cfg(feature = "swagger-ui-embedded")]
    download_swagger_ui();
}

#[cfg(feature = "swagger-ui-embedded")]
fn download_swagger_ui() {
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;

    const SWAGGER_UI_VERSION: &str = "5.17.14";

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let swagger_dir = out_dir.join("swagger-ui");

    // 如果已下载则跳过
    let marker = swagger_dir.join(".version");
    if marker.exists() {
        if let Ok(v) = fs::read_to_string(&marker) {
            if v.trim() == SWAGGER_UI_VERSION {
                return;
            }
        }
    }

    fs::create_dir_all(&swagger_dir).expect("无法创建 swagger-ui 目录");

    let base_url = format!("https://unpkg.com/swagger-ui-dist@{SWAGGER_UI_VERSION}");

    let files = [
        "swagger-ui-bundle.js",
        "swagger-ui-standalone-preset.js",
        "swagger-ui.css",
        "favicon-32x32.png",
        "favicon-16x16.png",
    ];

    let agent = ureq::agent();

    for file in &files {
        let url = format!("{base_url}/{file}");
        let dest = swagger_dir.join(file);

        println!("cargo:warning=下载 Swagger UI 资源: {file}");

        let resp = agent
            .get(&url)
            .call()
            .unwrap_or_else(|e| panic!("下载 {url} 失败: {e}"));

        let mut bytes = Vec::new();
        resp.into_reader()
            .read_to_end(&mut bytes)
            .unwrap_or_else(|e| panic!("读取 {url} 响应体失败: {e}"));

        fs::write(&dest, &bytes).unwrap_or_else(|e| panic!("写入 {} 失败: {e}", dest.display()));
    }

    fs::write(&marker, SWAGGER_UI_VERSION).expect("写入版本标记失败");
    println!("cargo:warning=Swagger UI {SWAGGER_UI_VERSION} 资源下载完成");
}
