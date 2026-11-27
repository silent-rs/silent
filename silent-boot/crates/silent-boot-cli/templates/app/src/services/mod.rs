//! 业务逻辑：查询、写入、事务、审计字段。入参是 DTO，错误是 `AppError`，不依赖 HTTP 的类型。

pub mod article;

// `generate module` 生成的模块登记在这个区块里。
// <silent-boot:gen:mods>
// </silent-boot:gen:mods>
