use crate::SilentError;
use uuid::Uuid;

/// 路由参数
/// 支持类型：
///     String(String)
///     Int(i32),
///     Int64(i64),
///     Int32(i32),
///     UInt64(u64),
///     UInt32(u32),
///     Uuid(Uuid),
///     Path(String),
/// 支持数据转换
#[derive(Debug, Clone, PartialEq)]
pub enum PathParam {
    /// 字符串类型参数
    String(String),
    /// 整型参数
    Int(i32),
    /// 整型参数
    Int64(i64),
    /// 整型参数
    Int32(i32),
    /// 整型参数
    UInt64(u64),
    /// 整型参数
    UInt32(u32),
    /// Uuid类型参数
    Uuid(Uuid),
    /// 路径参数
    Path(String),
}

impl From<String> for PathParam {
    fn from(s: String) -> Self {
        PathParam::String(s)
    }
}

impl From<i32> for PathParam {
    fn from(i: i32) -> Self {
        PathParam::Int(i)
    }
}

impl From<i64> for PathParam {
    fn from(i: i64) -> Self {
        PathParam::Int64(i)
    }
}

impl From<u64> for PathParam {
    fn from(i: u64) -> Self {
        PathParam::UInt64(i)
    }
}

impl From<u32> for PathParam {
    fn from(i: u32) -> Self {
        PathParam::UInt32(i)
    }
}

impl From<Uuid> for PathParam {
    fn from(u: Uuid) -> Self {
        PathParam::Uuid(u)
    }
}

impl<'a> TryFrom<&'a PathParam> for i32 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Int(value) => Ok(*value),
            PathParam::Int32(value) => Ok(*value),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for i64 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Int64(value) => Ok(*value),
            PathParam::Int(value) => Ok(i64::from(*value)),
            PathParam::Int32(value) => Ok(i64::from(*value)),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for u64 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::UInt64(value) => Ok(*value),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for u32 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::UInt32(value) => Ok(*value),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for String {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::String(value) => Ok(value.clone()),
            PathParam::Path(value) => Ok(value.clone()),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for Uuid {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Uuid(value) => Ok(*value),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}
