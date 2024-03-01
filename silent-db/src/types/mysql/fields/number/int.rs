use crate::core::fields::{Field, FieldType};
#[derive(Clone)]
pub struct Int {
    pub name: String,
    pub default: Option<String>,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub auto_increment: bool,
    pub comment: Option<String>,
}

impl Default for Int {
    fn default() -> Self {
        Int {
            name: "int".to_string(),
            default: None,
            nullable: true,
            primary_key: false,
            unique: false,
            auto_increment: false,
            comment: None,
        }
    }
}

struct IntType;

impl FieldType for IntType {
    fn get_type_str(&self) -> String {
        "INT".to_string()
    }
}

impl Field for Int {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_type(&self) -> Box<dyn FieldType> {
        Box::new(IntType)
    }
    fn get_default(&self) -> Option<String> {
        self.default.clone()
    }
    fn get_nullable(&self) -> bool {
        match self.primary_key {
            true => false,
            false => self.nullable,
        }
    }
    fn get_primary_key(&self) -> bool {
        self.primary_key
    }
    fn get_unique(&self) -> bool {
        match self.primary_key {
            true => true,
            false => self.unique,
        }
    }
    fn get_auto_increment(&self) -> bool {
        self.auto_increment
    }
    fn get_comment(&self) -> Option<String> {
        self.comment.clone()
    }
}
