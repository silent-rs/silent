//! `generate module` 的第一步：从一张表生成 dto、service、api 三个文件，并登记到应用的 `mod.rs`。
//!
//! 生成的代码和脚手架里的"文章"示例模块是同一个写法：api 取参数、调 service、返回包络；service 管查询、
//! 写入、审计字段和软删除；dto 管校验和出参。文件的归属见 `docs/design.md` §4 和 §10：
//! 生成之后归应用所有，再生成时不覆盖已有的文件（除非 `--force`）。
//!
//! 约定（都来自迁移执行之后的库结构）：
//! - 主键是字符串（scru128）或者自增整数；
//! - 有 `deleted_at` 列的表按软删除处理：查询时过滤，删除时写 `deleted_at`；
//! - `created_at`、`updated_at`、`created_by`、`updated_by` 由 service 写入，不出现在入参里；
//! - 暂不支持生成 DTO 字段的列（二进制、定点数、数组等）不进入 DTO，留给人手写。
//!
//! 目前只支持 SQLite。只生成默认库的表。

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sea_schema::sea_query::ColumnType;
use sea_schema::sea_query::StringLen;
use sea_schema::sqlite::def::{ColumnInfo, DefaultType, TableDef};

use super::generate::rustfmt;

/// `src/dto/mod.rs`、`src/services/mod.rs`、`src/api/mod.rs` 里登记模块的受管区块。
pub const MODS_BEGIN: &str = "// <silent-boot:gen:mods>";
/// 登记模块的受管区块终点。
pub const MODS_END: &str = "// </silent-boot:gen:mods>";
/// `src/api/mod.rs` 里登记路由的受管区块起点。
pub const ROUTES_BEGIN: &str = "// <silent-boot:gen:routes>";
/// 登记路由的受管区块终点。
pub const ROUTES_END: &str = "// </silent-boot:gen:routes>";

/// `generate module` 的可选参数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleOptions {
    /// 按哪张表生成，也是模块名。
    pub table: String,
    /// 已有的文件照样覆盖。默认不覆盖：生成出来的代码归应用所有。
    pub force: bool,
    /// 只列出会写出、会变化的文件，不写入。
    pub dry_run: bool,
}

/// 一次生成的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratedModule {
    /// 模块名，也是表名。
    pub module: String,
    /// 写出或者将要写出的文件，相对于应用根目录。
    pub files: Vec<PathBuf>,
    /// 因为已经存在而没有动的文件。
    pub skipped: Vec<PathBuf>,
}

/// 生成模块时的错误。
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    /// 暂不支持的数据库地址。
    #[error("暂不支持这个数据库地址，目前只支持 sqlite：{0}")]
    UnsupportedUrl(String),
    /// 连接或读取库结构失败。
    #[error("连接或读取库结构失败：{0}")]
    Discovery(String),
    /// 没有这张表。
    #[error("库里没有表 {table:?}。现有的表：{available:?}。先写迁移、执行迁移，再生成模块")]
    NoSuchTable {
        /// 要生成的表名。
        table: String,
        /// 库里现有的表。
        available: Vec<String>,
    },
    /// 表的结构不符合约定，第一步的生成器处理不了。
    #[error("表 {table} 的结构不符合约定：{reason}")]
    UnsupportedTable { table: String, reason: String },
    /// 没有 `mod.rs`：应用不是用脚手架生成的，或者目录结构变了。
    #[error("没有 {0}。按脚手架生成的目录结构，应用应该有这个文件")]
    MissingFile(PathBuf),
    /// `mod.rs` 里找不到受管区块的标记。
    #[error("{} 里找不到标记 {marker}。按脚手架生成的目录结构，这个文件应该带着受管区块", file.display())]
    MissingMarkers {
        /// 哪个文件。
        file: PathBuf,
        /// 缺的标记。
        marker: &'static str,
    },
    /// 生成或者格式化失败。
    #[error("生成模块失败：{0}")]
    Codegen(String),
    /// 读写文件失败。
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// 按 `app_root` 下的约定生成一个模块：`src/dto/<表>.rs`、`src/services/<表>.rs`、`src/api/<表>.rs`，
/// 并在三个 `mod.rs` 里登记。
pub async fn generate_module(
    database_url: &str,
    app_root: &Path,
    options: &ModuleOptions,
) -> Result<GeneratedModule, ModuleError> {
    let tables = discover(database_url).await?;
    let available = tables.iter().map(|table| table.name.clone()).collect();
    let table = tables
        .into_iter()
        .find(|table| table.name == options.table)
        .ok_or(ModuleError::NoSuchTable {
            available,
            table: options.table.clone(),
        })?;
    let model = Model::from_table(&table)?;
    let files = model.render()?;

    let mut generated = GeneratedModule {
        module: model.module.clone(),
        ..GeneratedModule::default()
    };
    // 先确认三个 `mod.rs` 都能登记，再写文件：登记不了时不留下没人引用的模块。
    let registrations = register(app_root, &model.module, options.dry_run)?;
    for (relative, content) in files {
        let path = app_root.join(&relative);
        let exists = path.exists();
        if exists && !options.force {
            generated.skipped.push(relative);
            continue;
        }
        generated.files.push(relative.clone());
        if options.dry_run {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &path,
            rustfmt(&content).map_err(|e| ModuleError::Codegen(e.to_string()))?,
        )?;
    }
    for path in registrations {
        if !generated.files.contains(&path) {
            generated.files.push(path);
        }
    }
    Ok(generated)
}

/// 发现库结构，顺带把"没有这张表"时的提示补全。
async fn discover(database_url: &str) -> Result<Vec<TableDef>, ModuleError> {
    let all = super::generate::discover(database_url)
        .await
        .map_err(|e| match e {
            super::generate::GenerateError::UnsupportedUrl(url) => ModuleError::UnsupportedUrl(url),
            super::generate::GenerateError::Discovery(message) => ModuleError::Discovery(message),
            other => ModuleError::Discovery(other.to_string()),
        })?;
    Ok(all
        .into_iter()
        .filter(|table| !super::generate::DEFAULT_IGNORED_TABLES.contains(&table.name.as_str()))
        .collect())
}

/// 一张表里生成代码要用到的信息。
struct Model {
    module: String,
    name: String,
    columns: Vec<Column>,
    primary: Column,
    auto_increment: bool,
    soft_delete: bool,
    keyword: Option<String>,
    audit: AuditColumns,
}

/// 审计字段里有哪些：有的话 service 负责写入。
struct AuditColumns {
    created_at: Option<String>,
    updated_at: Option<String>,
    created_by: Option<String>,
    updated_by: Option<String>,
}

#[derive(Debug, Clone)]
struct Column {
    name: String,
    rust: &'static str,
    /// 列允许为空：实体里是 `Option<T>`。
    nullable: bool,
    /// 列有默认值：入参里可以不传。
    default: Option<String>,
    /// 字符串列的长度上限，没有就是 `None`。
    max_length: Option<u32>,
}

impl Column {
    /// 入参里可以不传：列允许为空，或者有默认值。
    fn optional(&self) -> bool {
        self.nullable || self.default.is_some()
    }

    /// Rust 类型实现了 `Copy`：数字、布尔、日期时间。
    fn is_copy(&self) -> bool {
        !matches!(self.rust, "String" | "serde_json::Value")
    }

    /// 入参上的校验规则：必填的字符串不能是空的，有长度上限的不能超。
    fn validate_attr(&self) -> String {
        if self.rust != "String" {
            return String::new();
        }
        let (min, max) = match (self.optional(), self.max_length) {
            (true, None) => return String::new(),
            (true, Some(max)) => (None, Some(max)),
            (false, Some(max)) => (Some(1), Some(max)),
            (false, None) => (Some(1), None),
        };
        let mut rule = String::from("    #[validate(length(");
        if let Some(min) = min {
            let _ = write!(rule, "min = {min}");
        }
        if let Some(max) = max {
            if min.is_some() {
                rule.push_str(", ");
            }
            let _ = write!(rule, "max = {max}");
        }
        rule.push_str("))]\n");
        rule
    }
}

/// DTO 字段里的 Rust 类型。
fn rust_type(kind: &ColumnType) -> Option<&'static str> {
    use ColumnType as K;
    Some(match kind {
        K::TinyInteger => "i8",
        K::SmallInteger => "i16",
        K::Integer => "i32",
        K::BigInteger => "i64",
        K::TinyUnsigned => "u8",
        K::SmallUnsigned => "u16",
        K::Unsigned => "u32",
        K::BigUnsigned => "u64",
        K::Float => "f32",
        K::Double => "f64",
        K::Boolean => "bool",
        K::Char(_) | K::String(StringLen::N(_)) | K::String(StringLen::None) | K::Text => "String",
        K::Json | K::JsonBinary => "serde_json::Value",
        K::Date => "chrono::NaiveDate",
        K::DateTime | K::Timestamp => "chrono::NaiveDateTime",
        K::Enum { .. } => "String",
        _ => return None,
    })
}

/// 入参里的字段名。和 Rust 的关键字重名时用原始标识符。
fn field_name(column: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "else", "enum", "extern", "false", "fn", "for", "if",
        "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
        "static", "struct", "trait", "true", "type", "unsafe", "use", "where", "while", "async",
        "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
        "typeof", "unsized", "virtual", "yield", "try",
    ];
    if KEYWORDS.contains(&column) {
        format!("r#{column}")
    } else {
        column.to_owned()
    }
}

/// 表名的帕斯卡写法：`sys_user` 变成 `SysUser`。
fn pascal(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

impl Model {
    fn from_table(table: &TableDef) -> Result<Self, ModuleError> {
        let unsupported = |reason: String| ModuleError::UnsupportedTable {
            table: table.name.clone(),
            reason,
        };
        if !table
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || table.name.starts_with(|c: char| c.is_ascii_digit())
        {
            return Err(unsupported(format!(
                "表名 {:?} 不是合法的 Rust 标识符",
                table.name
            )));
        }
        let primaries: Vec<&ColumnInfo> =
            table.columns.iter().filter(|col| col.primary_key).collect();
        let [primary] = primaries[..] else {
            return Err(unsupported("需要恰好一个主键列".to_owned()));
        };
        let pk_type = rust_type(&primary.r#type);
        let auto_increment = table.auto_increment;
        let primary_rust = match pk_type {
            Some("String") if !auto_increment => "String",
            Some("i32") if auto_increment => "i32",
            _ => {
                return Err(unsupported(
                    "主键要么是不自增的字符串（scru128），要么是自增的 32 位整数".to_owned(),
                ));
            }
        };
        use sea_schema::sqlite::def::ColumnVisibility;
        if table.columns.iter().any(|col| {
            matches!(
                col.hidden,
                ColumnVisibility::GeneratedVirtual | ColumnVisibility::GeneratedStored
            )
        }) {
            return Err(unsupported("有生成列，第一步的生成器不处理".to_owned()));
        }

        let mut columns = Vec::new();
        for col in &table.columns {
            let Some(rust) = rust_type(&col.r#type) else {
                continue;
            };
            if col.primary_key {
                columns.push(Column {
                    name: col.name.clone(),
                    rust: primary_rust,
                    nullable: false,
                    default: None,
                    max_length: None,
                });
                continue;
            }
            let default = default_literal(&col.default_value, rust);
            columns.push(Column {
                name: col.name.clone(),
                rust,
                nullable: !col.not_null,
                default,
                max_length: max_length(&col.r#type),
            });
        }

        let find = |name: &str, rust: &str| {
            columns
                .iter()
                .find(|col| col.name == name && col.rust == rust)
                .map(|col| col.name.clone())
        };
        // 列表按第一个像"标题"的文本列做关键字过滤。
        let keyword = ["title", "name", "label"]
            .into_iter()
            .find(|candidate| {
                columns
                    .iter()
                    .any(|col| col.name == *candidate && col.rust == "String")
            })
            .map(str::to_owned);
        let audit = AuditColumns {
            created_at: find("created_at", "chrono::NaiveDateTime"),
            updated_at: find("updated_at", "chrono::NaiveDateTime"),
            created_by: find("created_by", "String"),
            updated_by: find("updated_by", "String"),
        };
        let primary = Column {
            name: primary.name.clone(),
            rust: primary_rust,
            nullable: false,
            default: None,
            max_length: None,
        };
        Ok(Self {
            module: table.name.clone(),
            name: pascal(&table.name),
            columns,
            primary,
            auto_increment,
            soft_delete: table.columns.iter().any(|col| col.name == "deleted_at"),
            keyword,
            audit,
        })
    }

    /// 入参里要填的列：除了主键和审计字段。
    fn inputs(&self) -> impl Iterator<Item = &Column> {
        const AUDIT: &[&str] = &[
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
        ];
        self.columns
            .iter()
            .filter(|col| col.name != self.primary.name && !AUDIT.contains(&col.name.as_str()))
    }

    /// 出参里的列：除了软删除标记。
    fn outputs(&self) -> impl Iterator<Item = &Column> {
        self.columns.iter().filter(|col| col.name != "deleted_at")
    }

    fn render(&self) -> Result<Vec<(PathBuf, String)>, ModuleError> {
        let files = [
            (
                PathBuf::from(format!("src/dto/{}.rs", self.module)),
                self.dto(),
            ),
            (
                PathBuf::from(format!("src/services/{}.rs", self.module)),
                self.service(),
            ),
            (
                PathBuf::from(format!("src/api/{}.rs", self.module)),
                self.api(),
            ),
        ];
        Ok(files.into_iter().collect())
    }

    fn dto(&self) -> String {
        let name = &self.name;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "//! `{}` 的入参和出参。由 `generate module --table {}` 生成，之后归应用所有。",
            self.module, self.module
        );
        let _ = writeln!(
            out,
            "\nuse entity::{};\nuse silent_boot::prelude::*;\n",
            self.module
        );

        let _ = writeln!(out, "/// 新建一条 `{}` 记录。", self.module);
        let _ = writeln!(
            out,
            "#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]"
        );
        let _ = writeln!(out, "#[serde(rename_all = \"camelCase\")]");
        let _ = writeln!(out, "pub struct Create{name}Dto {{");
        for col in self.inputs() {
            self.write_field_doc(&mut out, col, "新建时");
            let _ = write!(out, "{}", col.validate_attr());
            let _ = write!(out, "    pub {}: ", field_name(&col.name));
            if col.optional() {
                let _ = write!(out, "Option<{}>", col.rust);
            } else {
                let _ = write!(out, "{}", col.rust);
            }
            let _ = writeln!(out, ",");
        }
        let _ = writeln!(out, "}}\n");

        let _ = writeln!(out, "/// 修改一条 `{}` 记录。没传的字段不动。", self.module);
        let _ = writeln!(
            out,
            "#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]"
        );
        let _ = writeln!(out, "#[serde(rename_all = \"camelCase\")]");
        let _ = writeln!(out, "pub struct Update{name}Dto {{");
        for col in self.inputs() {
            self.write_field_doc(&mut out, col, "修改时");
            let _ = write!(out, "{}", col.validate_attr());
            let _ = writeln!(
                out,
                "    pub {}: Option<{}>,",
                field_name(&col.name),
                col.rust
            );
        }
        let _ = writeln!(out, "}}\n");

        let _ = writeln!(
            out,
            "/// 列表的查询条件。分页参数由 `Page` 提取器单独取，这里只有业务上的过滤条件。"
        );
        let _ = writeln!(
            out,
            "#[derive(Debug, Deserialize, Validate, utoipa::IntoParams)]"
        );
        let _ = writeln!(out, "#[into_params(parameter_in = Query)]");
        let _ = writeln!(out, "pub struct {name}QueryDto {{");
        if let Some(keyword) = &self.keyword {
            let _ = writeln!(
                out,
                "    /// `{keyword}` 包含的关键字。\n    #[validate(length(max = 100))]\n    pub keyword: Option<String>,"
            );
        }
        let _ = writeln!(out, "}}\n");

        let _ = writeln!(out, "/// 一条 `{}` 记录的出参。", self.module);
        let _ = writeln!(out, "#[derive(Debug, Serialize, utoipa::ToSchema)]");
        let _ = writeln!(out, "#[serde(rename_all = \"camelCase\")]");
        let _ = writeln!(out, "pub struct {name}Dto {{");
        for col in self.outputs() {
            let _ = writeln!(
                out,
                "    pub {}: {},",
                field_name(&col.name),
                self.output_type(col)
            );
        }
        let _ = writeln!(out, "}}\n");
        let _ = writeln!(
            out,
            "impl From<{module}::Model> for {name}Dto {{",
            module = self.module
        );
        let _ = writeln!(out, "    fn from(model: {}::Model) -> Self {{", self.module);
        let _ = writeln!(out, "        Self {{");
        for col in self.outputs() {
            let _ = writeln!(
                out,
                "            {}: model.{},",
                field_name(&col.name),
                field_name(&col.name)
            );
        }
        let _ = writeln!(out, "        }}");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "}}");
        out
    }

    fn write_field_doc(&self, out: &mut String, col: &Column, when: &str) {
        let _ = writeln!(out, "    /// `{}` 列。", col.name);
        let _ = writeln!(
            out,
            "    ///\n    /// {when}{}。",
            if col.optional() {
                if let Some(default) = &col.default {
                    format!("可以不传，不传时用默认值 {default}")
                } else {
                    "可以不传".to_owned()
                }
            } else {
                "必须给".to_owned()
            }
        );
    }

    fn output_type(&self, col: &Column) -> String {
        // 自增主键在插入之后才有值；其余按列的可空性。
        if col.nullable {
            format!("Option<{}>", col.rust)
        } else {
            col.rust.to_owned()
        }
    }

    fn service(&self) -> String {
        let name = &self.name;
        let module = &self.module;
        let pk = &self.primary;
        let pk_field = field_name(&pk.name);
        let soft = self.soft_delete;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "//! `{module}` 的业务逻辑：查询、新建、修改、删除。由 `generate module --table {module}` 生成，之后归应用所有。"
        );
        let _ = writeln!(
            out,
            "//!\n//! 审计字段（`created_at`、`updated_at`、`created_by`、`updated_by`）在这里写入：\n//! SeaORM 不会自动填充，实体的文件也不手改。\n"
        );
        let _ = writeln!(out, "use entity::{module};");
        if soft {
            let _ = writeln!(out, "use sea_orm::sea_query::Expr;");
        }
        let _ = writeln!(
            out,
            "use sea_orm::{{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set}};"
        );
        let _ = writeln!(out, "use silent_boot::prelude::*;\n");
        let _ = writeln!(
            out,
            "use crate::dto::{module}::{{{name}Dto, {name}QueryDto, Create{name}Dto, Update{name}Dto}};\n"
        );
        let _ = writeln!(out, "pub struct {name}Service;\n");
        let _ = writeln!(
            out,
            "/// 当前用户的 ID。接上认证之后换成从请求里取到的值。\nconst SYSTEM_USER: &str = \"system\";\n"
        );

        // page
        let _ = writeln!(out, "impl {name}Service {{");
        let _ = writeln!(
            out,
            "    /// 分页查询{}。",
            if soft {
                "，只返回没删除的记录"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "    pub async fn page(\n        db: &sea_orm::DatabaseConnection,\n        page: &PageQuery,\n        query: &{name}QueryDto,\n    ) -> AppResult<PageResult<{name}Dto>> {{"
        );
        let _ = writeln!(out, "        let mut select = {module}::Entity::find();");
        if soft {
            let _ = writeln!(
                out,
                "        select = select.filter({module}::Column::DeletedAt.is_null());"
            );
        }
        if let Some(keyword) = &self.keyword {
            let column = pascal(keyword);
            let _ = writeln!(
                out,
                "        if let Some(keyword) = query.keyword.as_deref().filter(|k| !k.is_empty()) {{\n            select = select.filter({module}::Column::{column}.contains(keyword));\n        }}"
            );
        }
        let _ = match &self.audit.created_at {
            Some(_) => writeln!(
                out,
                "        let select = select.order_by_desc({module}::Column::CreatedAt);"
            ),
            None => writeln!(
                out,
                "        let select = select.order_by_asc({module}::Column::{});",
                pascal(&pk.name)
            ),
        };
        let _ = writeln!(
            out,
            "        Ok(paginate(db, select, page, {name}Dto::from).await?)\n    }}\n"
        );

        // get
        let _ = writeln!(
            out,
            "    /// 按主键取一条{}。",
            if soft { "，删除的不算" } else { "" }
        );
        let _ = writeln!(
            out,
            "    pub async fn get(db: &sea_orm::DatabaseConnection, {pk_field}: &{pk_type}) -> AppResult<{name}Dto> {{\n        existing(db, {pk_field}).await.map({name}Dto::from)\n    }}\n",
            pk_type = pk.rust
        );

        // create
        let _ = writeln!(
            out,
            "    pub async fn create(db: &sea_orm::DatabaseConnection, input: &Create{name}Dto) -> AppResult<{name}Dto> {{"
        );
        let now_needed = self.audit.created_at.is_some() || self.audit.updated_at.is_some() || soft;
        if now_needed {
            let _ = writeln!(out, "        let now = chrono::Local::now().naive_local();");
        }
        let _ = writeln!(out, "        let model = {module}::ActiveModel {{");
        if self.auto_increment {
            let _ = writeln!(out, "            {pk_field}: sea_orm::ActiveValue::NotSet,");
        } else {
            let _ = writeln!(out, "            {pk_field}: Set(scru128::new_string()),");
        }
        for col in self.inputs() {
            let f = field_name(&col.name);
            // 实现了 `Copy` 的类型不必 `clone()`，clippy 会拦。
            let value = if col.is_copy() {
                format!("input.{f}")
            } else {
                format!("input.{f}.clone()")
            };
            if let Some(default) = &col.default {
                let _ = writeln!(out, "            {f}: Set({value}.unwrap_or({default})),");
            } else {
                let _ = writeln!(out, "            {f}: Set({value}),");
            }
        }
        if self.audit.created_at.is_some() {
            let _ = writeln!(out, "            created_at: Set(now),");
        }
        if self.audit.updated_at.is_some() {
            let _ = writeln!(out, "            updated_at: Set(now),");
        }
        if self.audit.created_by.is_some() {
            let _ = writeln!(
                out,
                "            created_by: Set(Some(SYSTEM_USER.to_owned())),"
            );
        }
        if self.audit.updated_by.is_some() {
            let _ = writeln!(
                out,
                "            updated_by: Set(Some(SYSTEM_USER.to_owned())),"
            );
        }
        let _ = writeln!(out, "            ..Default::default()");
        let _ = writeln!(out, "        }}");
        let _ = writeln!(out, "        .insert(db)");
        let _ = writeln!(out, "        .await?;");
        let _ = writeln!(out, "        Ok(model.into())");
        let _ = writeln!(out, "    }}\n");

        // update
        let _ = writeln!(out, "    /// 没传的字段不动。",);
        let _ = writeln!(
            out,
            "    pub async fn update(\n        db: &sea_orm::DatabaseConnection,\n        {pk_field}: &{pk_type},\n        input: &Update{name}Dto,\n    ) -> AppResult<{name}Dto> {{\n        let mut model: {module}::ActiveModel = existing(db, {pk_field}).await?.into();",
            pk_type = pk.rust
        );
        for col in self.inputs() {
            let f = field_name(&col.name);
            if col.nullable {
                if col.is_copy() {
                    let _ = writeln!(
                        out,
                        "        if let Some({f}) = input.{f} {{\n            model.{f} = Set(Some({f}));\n        }}"
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "        if let Some({f}) = &input.{f} {{\n            model.{f} = Set(Some({f}.clone()));\n        }}"
                    );
                }
            } else if col.is_copy() {
                let _ = writeln!(
                    out,
                    "        if let Some({f}) = input.{f} {{\n            model.{f} = Set({f});\n        }}"
                );
            } else {
                let _ = writeln!(
                    out,
                    "        if let Some({f}) = input.{f}.clone() {{\n            model.{f} = Set({f});\n        }}"
                );
            }
        }
        if self.audit.updated_at.is_some() {
            let _ = writeln!(
                out,
                "        model.updated_at = Set(chrono::Local::now().naive_local());"
            );
        }
        if self.audit.updated_by.is_some() {
            let _ = writeln!(
                out,
                "        model.updated_by = Set(Some(SYSTEM_USER.to_owned()));"
            );
        }
        let _ = writeln!(out, "        Ok(model.update(db).await?.into())");
        let _ = writeln!(out, "    }}");

        // remove
        if soft {
            let _ = writeln!(out, "\n    /// 软删除：写 `deleted_at`，不删行。");
            let _ = writeln!(
                out,
                "    pub async fn remove(db: &sea_orm::DatabaseConnection, {pk_field}: &{pk_type}) -> AppResult<()> {{\n        let result = {module}::Entity::update_many()\n            .col_expr(\n                {module}::Column::DeletedAt,\n                Expr::value(chrono::Local::now().naive_local()),\n            )",
                pk_type = pk.rust
            );
            if self.audit.updated_by.is_some() {
                let _ = writeln!(
                    out,
                    "            .col_expr(\n                {module}::Column::UpdatedBy,\n                Expr::value(Some(SYSTEM_USER.to_owned())),\n            )"
                );
            }
            let _ = writeln!(
                out,
                "            .filter({module}::Column::{pk_column}.eq({pk_field}))\n            .filter({module}::Column::DeletedAt.is_null())\n            .exec(db)\n            .await?;",
                pk_column = pascal(&pk.name)
            );
        } else {
            let _ = writeln!(out, "\n    /// 删除一行。");
            let _ = writeln!(
                out,
                "    pub async fn remove(db: &sea_orm::DatabaseConnection, {pk_field}: &{pk_type}) -> AppResult<()> {{\n        let result = {module}::Entity::delete_many()\n            .filter({module}::Column::{pk_column}.eq({pk_field}))\n            .exec(db)\n            .await?;",
                pk_type = pk.rust,
                pk_column = pascal(&pk.name)
            );
        }
        let _ = writeln!(
            out,
            "        if result.rows_affected == 0 {{\n            return Err(AppError::not_found(\"{module}\"));\n        }}\n        Ok(())\n    }}\n}}\n"
        );

        let _ = writeln!(
            out,
            "/// 按主键取一条{}，取不到就是 404。\nasync fn existing(\n    db: &sea_orm::DatabaseConnection,\n    {pk_field}: &{pk_type},\n) -> AppResult<{module}::Model> {{\n    {module}::Entity::find_by_id({pk_field})",
            if soft { "没删除的记录" } else { "记录" },
            pk_type = pk.rust
        );
        if soft {
            let _ = writeln!(
                out,
                "        .filter({module}::Column::DeletedAt.is_null())"
            );
        }
        let _ = writeln!(
            out,
            "        .one(db)\n        .await?\n        .ok_or_else(|| AppError::not_found(\"{module}\"))\n}}"
        );
        out
    }

    fn api(&self) -> String {
        // 路径用复数，和示例模块一致：`article` 对应 `/api/articles`。表名已经以 s 结尾就不加。
        let path = if self.module.ends_with('s') {
            self.module.clone()
        } else {
            format!("{}s", self.module)
        };
        let name = &self.name;
        let module = &self.module;
        let pk = &self.primary;
        let pk_field = field_name(&pk.name);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "//! `{module}` 的接口：从请求里取参数，调用 service，返回包络。由 `generate module --table {module}` 生成，之后归应用所有。"
        );
        let _ = writeln!(out, "//!\n//! 这里不写业务逻辑，也不直接操作实体。\n");
        let _ = writeln!(out, "use silent_boot::prelude::*;\n");
        let _ = writeln!(
            out,
            "use crate::dto::{module}::{{{name}Dto, {name}QueryDto, Create{name}Dto, Update{name}Dto}};\nuse crate::services::{module}::{name}Service;\n"
        );
        let _ = writeln!(
            out,
            "pub fn route() -> Route {{\n    Route::new(\"{path}\")\n        .get(page)\n        .post(create)\n        .append(Route::new(\"<{pk_field}>\").get(detail).put(update).delete(remove))\n}}\n"
        );
        let params = match &self.keyword {
            Some(_) => format!("{name}QueryDto, PageQuery"),
            None => "PageQuery".to_owned(),
        };
        let _ = writeln!(
            out,
            "#[utoipa::path(\n    get,\n    path = \"/api/{path}\",\n    params({params}),\n    responses((status = 200, body = Envelope<PageResult<{name}Dto>>))\n)]"
        );
        let _ = writeln!(
            out,
            "async fn page(\n    (State(app), page, ValidatedQuery(query)): (State<AppState>, Page, ValidatedQuery<{name}QueryDto>),\n) -> Result<Envelope<PageResult<{name}Dto>>> {{\n    Ok(Envelope::ok(\n        {name}Service::page(app.db.db(), &page.0, &query).await?,\n    ))\n}}\n"
        );
        let _ = writeln!(
            out,
            "#[utoipa::path(\n    post,\n    path = \"/api/{path}\",\n    request_body = Create{name}Dto,\n    responses(\n        (status = 201, body = Envelope<{name}Dto>),\n        (status = 422, description = \"参数校验失败，data 里是字段错误\")\n    )\n)]"
        );
        let _ = writeln!(
            out,
            "async fn create(\n    (State(app), ValidatedJson(input)): (State<AppState>, ValidatedJson<Create{name}Dto>),\n) -> Result<Created<{name}Dto>> {{\n    Ok(Created(Envelope::created(\n        {name}Service::create(app.db.db(), &input).await?,\n    )))\n}}\n"
        );
        let path_param = format!(
            "(\"{}\" = {}, Path, description = \"主键\")",
            pk_field, pk.rust
        );
        let _ = writeln!(
            out,
            "#[utoipa::path(\n    get,\n    path = \"/api/{path}/{{{pk_field}}}\",\n    params({path_param}),\n    responses((status = 200, body = Envelope<{name}Dto>), (status = 404))\n)]"
        );
        let _ = writeln!(
            out,
            "async fn detail(\n    (State(app), Path({pk_field})): (State<AppState>, Path<{}>),\n) -> Result<Envelope<{name}Dto>> {{\n    Ok(Envelope::ok(\n        {name}Service::get(app.db.db(), &{pk_field}).await?,\n    ))\n}}\n",
            pk.rust
        );
        let _ = writeln!(
            out,
            "#[utoipa::path(\n    put,\n    path = \"/api/{path}/{{{pk_field}}}\",\n    params({path_param}),\n    request_body = Update{name}Dto,\n    responses((status = 200, body = Envelope<{name}Dto>), (status = 404), (status = 422))\n)]"
        );
        let _ = writeln!(
            out,
            "async fn update(\n    (State(app), Path({pk_field}), ValidatedJson(input)): (\n        State<AppState>,\n        Path<{}>,\n        ValidatedJson<Update{name}Dto>,\n    ),\n) -> Result<Envelope<{name}Dto>> {{\n    Ok(Envelope::ok(\n        {name}Service::update(app.db.db(), &{pk_field}, &input).await?,\n    ))\n}}\n",
            pk.rust
        );
        let _ = writeln!(
            out,
            "#[utoipa::path(\n    delete,\n    path = \"/api/{path}/{{{pk_field}}}\",\n    params({path_param}),\n    responses((status = 200, description = \"已删除，data 为 null\"), (status = 404))\n)]"
        );
        let _ = writeln!(
            out,
            "async fn remove(\n    (State(app), Path({pk_field})): (State<AppState>, Path<{}>),\n) -> Result<Envelope<serde_json::Value>> {{\n    {name}Service::remove(app.db.db(), &{pk_field}).await?;\n    Ok(Envelope::ok(serde_json::Value::Null))\n}}",
            pk.rust
        );
        out
    }
}

/// 字符串列的长度上限。
fn max_length(kind: &ColumnType) -> Option<u32> {
    match kind {
        ColumnType::String(StringLen::N(n)) | ColumnType::Char(Some(n)) => Some(*n),
        _ => None,
    }
}

/// 列的默认值写成 Rust 字面量；写不出来的返回 `None`。
fn default_literal(default: &DefaultType, rust: &str) -> Option<String> {
    match (default, rust) {
        (DefaultType::Integer(value), _) if rust.starts_with('i') || rust.starts_with('u') => {
            Some(value.to_string())
        }
        (DefaultType::Float(value), "f32") => Some(format!("{value}")),
        (DefaultType::Float(value), "f64") => Some(format!("{value}")),
        (DefaultType::String(value), "String") => Some(format!("{value:?}")),
        _ => None,
    }
}

/// 在三个 `mod.rs` 里登记模块。返回改动过（或者将要改动）的文件。
fn register(app_root: &Path, module: &str, dry_run: bool) -> Result<Vec<PathBuf>, ModuleError> {
    let mut changed = Vec::new();
    for relative in ["src/dto/mod.rs", "src/services/mod.rs", "src/api/mod.rs"] {
        let path = app_root.join(relative);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(ModuleError::MissingFile(PathBuf::from(relative)));
            }
            Err(e) => return Err(e.into()),
        };
        let mut next = insert_sorted(
            &source,
            MODS_BEGIN,
            MODS_END,
            &format!("pub mod {module};"),
            &path,
        )?;
        if relative == "src/api/mod.rs" {
            next = insert_sorted(
                &next,
                ROUTES_BEGIN,
                ROUTES_END,
                &format!(".append({module}::route())"),
                &path,
            )?;
        }
        if next != source {
            changed.push(PathBuf::from(relative));
            if !dry_run {
                fs::write(&path, next)?;
            }
        }
    }
    Ok(changed)
}

/// 在 `begin` 和 `end` 两行之间登记一条，保持区块内按名称排序，缩进沿用起点那一行。
fn insert_sorted(
    source: &str,
    begin: &'static str,
    end: &'static str,
    entry: &str,
    file: &Path,
) -> Result<String, ModuleError> {
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let find = |marker: &'static str| {
        lines
            .iter()
            .position(|line| line.trim() == marker)
            .ok_or(ModuleError::MissingMarkers {
                file: file.to_path_buf(),
                marker,
            })
    };
    let start = find(begin)?;
    let stop = find(end)?;
    if stop <= start {
        return Err(ModuleError::MissingMarkers {
            file: file.to_path_buf(),
            marker: end,
        });
    }
    let indent = lines[start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let mut entries: Vec<String> = lines[start + 1..stop]
        .iter()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect();
    if !entries.iter().any(|existing| existing == entry) {
        entries.push(entry.to_owned());
        entries.sort();
    }
    let formatted: Vec<String> = entries
        .iter()
        .map(|existing| format!("{indent}{existing}"))
        .collect();
    lines.splice(start + 1..stop, formatted);
    let mut content = lines.join("\n");
    content.push('\n');
    Ok(content)
}
