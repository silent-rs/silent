//! 业务接口。每个资源一个文件，在 [`routes`] 里挂到 `/api` 下。

use silent_boot::prelude::*;

// `generate module` 生成的模块登记在这两个区块里；手写的模块写在区块外面。
// <silent-boot:gen:mods>
// </silent-boot:gen:mods>
pub mod article;

/// 所有业务接口。
pub fn routes() -> Route {
    Route::new("api").append(article::route())
    // <silent-boot:gen:routes>
    // </silent-boot:gen:routes>
}
