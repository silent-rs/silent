use silent::prelude::*;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("")
        .with_state(1i32)
        .get(|req: Request| async move {
            let num = req.get_state::<i32>()?;
            Ok(*num)
        })
        .append(Route::new("check").get(|req: Request| async move {
            let num: &i64 = req.get_state()?;
            Ok(*num)
        }))
        .append(Route::new("uncheck").get(|req: Request| async move {
            let num: &i32 = req.get_state_uncheck();
            Ok(*num)
        }));
    Server::new().run(route);
}
