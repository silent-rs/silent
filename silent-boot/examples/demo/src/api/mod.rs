//! 业务接口。每个资源一个文件，在 [`routes`] 里挂到 `/api` 下。

use silent_boot::prelude::*;

// <silent-boot:gen:mods>
pub mod article;
// </silent-boot:gen:mods>

/// 所有业务接口。`generate module` 生成的模块登记在受管区块里。
pub fn routes() -> Route {
    Route::new("api").append(article::route())
    // <silent-boot:gen:routes>
    // </silent-boot:gen:routes>
}
