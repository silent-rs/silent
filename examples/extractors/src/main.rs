use serde::Deserialize;
use silent::prelude::*;

#[derive(Deserialize)]
struct Page {
    page: u32,
    size: u32,
}

async fn user_detail((Path(id), Query(p)): (Path<i64>, Query<Page>)) -> Result<String> {
    Ok(format!("id={id}, page={}, size={}", p.page, p.size))
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
    age: u32,
}

async fn create_user(Json(input): Json<CreateUser>) -> Result<String> {
    Ok(format!("created: {} ({})", input.name, input.age))
}

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("").append(
        Route::new("api/users")
            .post_ex::<Json<CreateUser>, _, _, _>(create_user)
            .append(
                Route::new("<id:int>").get_ex::<(Path<i64>, Query<Page>), _, _, _>(user_detail),
            ),
    );
    Server::new().run(route);
}
