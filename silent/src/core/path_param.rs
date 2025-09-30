use crate::SilentError;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use uuid::Uuid;

/// 零拷贝路由参数：字符串段会借用到共享路径缓冲，避免重复分配。
#[derive(Debug, Clone, PartialEq)]
pub enum PathParam {
    /// 普通字符串参数 `<key>` / `<key:str>`
    Str(PathString),
    /// 整型参数
    Int(i32),
    /// 64 位整型参数
    Int64(i64),
    /// 32 位整型参数（语义与 Int 相同，保留向后兼容）
    Int32(i32),
    /// 无符号 64 位参数
    UInt64(u64),
    /// 无符号 32 位参数
    UInt32(u32),
    /// Uuid 参数
    Uuid(Uuid),
    /// 通配路径参数 `<key:path>` / `<key:*>` / `<key:**>`
    Path(PathString),
}

impl PathParam {
    pub(crate) fn borrowed_str(source: Arc<str>, range: Range<usize>) -> Self {
        PathParam::Str(PathString::borrowed(source, range))
    }

    pub(crate) fn borrowed_path(source: Arc<str>, range: Range<usize>) -> Self {
        PathParam::Path(PathString::borrowed(source, range))
    }
}

impl From<String> for PathParam {
    fn from(value: String) -> Self {
        PathParam::Str(PathString::Owned(value))
    }
}

impl From<i32> for PathParam {
    fn from(value: i32) -> Self {
        PathParam::Int(value)
    }
}

impl From<i64> for PathParam {
    fn from(value: i64) -> Self {
        PathParam::Int64(value)
    }
}

impl From<u64> for PathParam {
    fn from(value: u64) -> Self {
        PathParam::UInt64(value)
    }
}

impl From<u32> for PathParam {
    fn from(value: u32) -> Self {
        PathParam::UInt32(value)
    }
}

impl From<Uuid> for PathParam {
    fn from(value: Uuid) -> Self {
        PathParam::Uuid(value)
    }
}

impl PathParam {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn path_owned(value: String) -> Self {
        PathParam::Path(PathString::Owned(value))
    }
}

impl<'a> TryFrom<&'a PathParam> for i32 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Int(v) => Ok(*v),
            PathParam::Int32(v) => Ok(*v),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for i64 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Int64(v) => Ok(*v),
            PathParam::Int(v) => Ok(i64::from(*v)),
            PathParam::Int32(v) => Ok(i64::from(*v)),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for u64 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::UInt64(v) => Ok(*v),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for u32 {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::UInt32(v) => Ok(*v),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for String {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Str(v) | PathParam::Path(v) => Ok(v.as_str().to_owned()),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

impl<'a> TryFrom<&'a PathParam> for Uuid {
    type Error = SilentError;

    fn try_from(value: &'a PathParam) -> Result<Self, Self::Error> {
        match value {
            PathParam::Uuid(v) => Ok(*v),
            _ => Err(SilentError::ParamsNotFound),
        }
    }
}

/// 字符串参数的持有方式，支持借用路径缓冲或拥有独立字符串。
#[derive(Debug, Clone, PartialEq)]
pub enum PathString {
    Borrowed(PathSlice),
    Owned(String),
}

impl PathString {
    pub(crate) fn borrowed(source: Arc<str>, range: Range<usize>) -> Self {
        PathString::Borrowed(PathSlice { source, range })
    }

    pub fn as_str(&self) -> &str {
        match self {
            PathString::Borrowed(slice) => slice.as_str(),
            PathString::Owned(value) => value.as_str(),
        }
    }

    pub fn as_cow(&self) -> Cow<'_, str> {
        match self {
            PathString::Borrowed(slice) => Cow::Borrowed(slice.as_str()),
            PathString::Owned(value) => Cow::Borrowed(value.as_str()),
        }
    }
}

/// 共享路径切片，用一个 `Arc<str>` + range 表示借用的子串。
#[derive(Debug, Clone, PartialEq)]
pub struct PathSlice {
    source: Arc<str>,
    range: Range<usize>,
}

impl PathSlice {
    pub fn as_str(&self) -> &str {
        &self.source[self.range.clone()]
    }

    pub fn source(&self) -> &Arc<str> {
        &self.source
    }

    pub fn range(&self) -> &Range<usize> {
        &self.range
    }
}
