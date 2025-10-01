use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let mut configs = Configs::default();
    configs.insert(1i32);
    let mut route = Route::new("")
        .get(|req: Request| async move {
            let num = req.get_config::<i32>()?;
            Ok(*num)
        })
        .append(Route::new("check").get(|req: Request| async move {
            let num: &i64 = req.get_config()?;
            Ok(*num)
        }))
        .append(Route::new("uncheck").get(|req: Request| async move {
            let num: &i32 = req.get_config_uncheck();
            Ok(*num)
        }));
    route.set_configs(Some(configs));
    Server::new().run(route);
}
