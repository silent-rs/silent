//! 集成测试工具
//!
//! 提供 `TestClient` 用于在不启动真实服务器的情况下测试路由、中间件和处理器。
//!
//! # 示例
//!
//! ```rust
//! use silent::prelude::*;
//! use silent::testing::TestClient;
//!
//! # async fn example() -> Result<()> {
//! let route = Route::new("hello").get(|_req: Request| async { Ok("Hello!") });
//! let app = Route::new_root().append(route);
//!
//! let resp = TestClient::get("/hello").send(&app).await;
//! assert_eq!(resp.status(), StatusCode::OK);
//! assert_eq!(resp.text().await, "Hello!");
//! # Ok(())
//! # }
//! ```

mod client;
mod response;

pub use client::{TestClient, TestRequest};
pub use response::TestResponse;
