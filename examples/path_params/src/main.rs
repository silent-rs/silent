use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    // 定义路由
    let route = Route::new("path_params")
        .append(Route::new("<key:str>/str").get(hello_world))
        .append(Route::new("<key:int>/int").get(hello_world))
        .append(Route::new("<key:uuid>/uuid").get(hello_world))
        .append(Route::new("<key:path>/path").get(hello_world))
        .append(Route::new("<key:full_path>/full_path").get(hello_world))
        .append(Route::new("<key:*>/*").get(hello_world))
        .append(Route::new("<key:**>/**").get(hello_world))
        .append(Route::new("<key>").get(hello_world))
        .append(Route::new("<key:other>/other").get(hello_world));
    info!("{:?}", route);
    Server::new().run(route);
}

// 定义处理方法
async fn hello_world(req: Request) -> Result<String> {
    // let path_params = req.get_path_params::<String>("key")?;
    // match path_params {
    //     PathParam::Str(str) => Ok(format!("str {}", str)),
    //     PathParam::Int(int) => Ok(format!("int {}", int)),
    //     PathParam::Uuid(uuid) => Ok(format!("uuid {}", uuid)),
    //     PathParam::Path(path) => Ok(format!("path {}", path)),
    // }
    req.get_path_params::<String>("key")
}
