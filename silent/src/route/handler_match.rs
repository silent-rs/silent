// 仅保留路由特殊段解析（例如 <id:i64>、<path:**>）。
// 具体匹配逻辑已迁移至 RouteTree。

pub(crate) enum SpecialPath {
    String(String),
    Int(String),
    I64(String),
    I32(String),
    U64(String),
    U32(String),
    UUid(String),
    Path(String),
    FullPath(String),
}

impl From<&str> for SpecialPath {
    fn from(value: &str) -> Self {
        // 去除首尾的尖括号
        let value = &value[1..value.len() - 1];
        let mut type_str = value.splitn(2, ':');
        let key = type_str.next().unwrap_or("");
        let path_type = type_str.next().unwrap_or("");
        match path_type {
            "**" => SpecialPath::FullPath(key.to_string()),
            "*" => SpecialPath::Path(key.to_string()),
            "full_path" => SpecialPath::FullPath(key.to_string()),
            "path" => SpecialPath::Path(key.to_string()),
            "str" => SpecialPath::String(key.to_string()),
            "int" => SpecialPath::Int(key.to_string()),
            "i64" => SpecialPath::I64(key.to_string()),
            "i32" => SpecialPath::I32(key.to_string()),
            "u64" => SpecialPath::U64(key.to_string()),
            "u32" => SpecialPath::U32(key.to_string()),
            "uuid" => SpecialPath::UUid(key.to_string()),
            _ => SpecialPath::String(key.to_string()),
        }
    }
}
