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

#[cfg(test)]
mod tests {
    use super::*;

    // PathParam::From<String> 测试
    #[test]
    fn test_path_param_from_string() {
        let s = String::from("hello");
        let param: PathParam = s.clone().into();
        assert!(matches!(param, PathParam::Str(PathString::Owned(_))));
        if let PathParam::Str(PathString::Owned(value)) = param {
            assert_eq!(value, s);
        }
    }

    // PathParam::From<i32> 测试
    #[test]
    fn test_path_param_from_i32() {
        let value: i32 = 42;
        let param: PathParam = value.into();
        assert_eq!(param, PathParam::Int(42));
    }

    // PathParam::From<i64> 测试
    #[test]
    fn test_path_param_from_i64() {
        let value: i64 = 123456789;
        let param: PathParam = value.into();
        assert_eq!(param, PathParam::Int64(123456789));
    }

    // PathParam::From<u64> 测试
    #[test]
    fn test_path_param_from_u64() {
        let value: u64 = 9876543210;
        let param: PathParam = value.into();
        assert_eq!(param, PathParam::UInt64(9876543210));
    }

    // PathParam::From<u32> 测试
    #[test]
    fn test_path_param_from_u32() {
        let value: u32 = 12345;
        let param: PathParam = value.into();
        assert_eq!(param, PathParam::UInt32(12345));
    }

    // PathParam::From<Uuid> 测试
    #[test]
    fn test_path_param_from_uuid() {
        let uuid = Uuid::nil();
        let param: PathParam = uuid.into();
        assert_eq!(param, PathParam::Uuid(uuid));
    }

    // PathParam::borrowed_str 测试
    #[test]
    fn test_path_param_borrowed_str() {
        let source = Arc::from("/users/123/profile");
        let range = 7..10; // "123"
        let param = PathParam::borrowed_str(source, range);
        assert!(matches!(param, PathParam::Str(PathString::Borrowed(_))));
        if let PathParam::Str(PathString::Borrowed(slice)) = param {
            assert_eq!(slice.as_str(), "123");
        }
    }

    // PathParam::borrowed_path 测试
    #[test]
    fn test_path_param_borrowed_path() {
        let source: Arc<str> = Arc::from("/files/path/to/file.txt");
        let range = 7..23; // "path/to/file.txt" (16 characters)
        let param = PathParam::borrowed_path(source, range);
        assert!(matches!(param, PathParam::Path(PathString::Borrowed(_))));
        if let PathParam::Path(PathString::Borrowed(slice)) = param {
            assert_eq!(slice.as_str(), "path/to/file.txt");
        }
    }

    // TryFrom<&PathParam> for i32 测试
    #[test]
    fn test_try_from_path_param_to_i32_from_int() {
        let param = PathParam::Int(42);
        let result: Result<i32, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_try_from_path_param_to_i32_from_int32() {
        let param = PathParam::Int32(100);
        let result: Result<i32, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_try_from_path_param_to_i32_from_int64() {
        let param = PathParam::Int64(200);
        let result: Result<i32, _> = (&param).try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_try_from_path_param_to_i32_from_str() {
        let param = PathParam::Str(PathString::Owned("hello".to_string()));
        let result: Result<i32, _> = (&param).try_into();
        assert!(result.is_err());
    }

    // TryFrom<&PathParam> for i64 测试
    #[test]
    fn test_try_from_path_param_to_i64_from_int64() {
        let param = PathParam::Int64(123456789);
        let result: Result<i64, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 123456789);
    }

    #[test]
    fn test_try_from_path_param_to_i64_from_int() {
        let param = PathParam::Int(42);
        let result: Result<i64, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_try_from_path_param_to_i64_from_int32() {
        let param = PathParam::Int32(100);
        let result: Result<i64, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_try_from_path_param_to_i64_from_uint64() {
        let param = PathParam::UInt64(123);
        let result: Result<i64, _> = (&param).try_into();
        assert!(result.is_err());
    }

    // TryFrom<&PathParam> for u64 测试
    #[test]
    fn test_try_from_path_param_to_u64_from_uint64() {
        let param = PathParam::UInt64(9876543210);
        let result: Result<u64, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 9876543210);
    }

    #[test]
    fn test_try_from_path_param_to_u64_from_int() {
        let param = PathParam::Int(42);
        let result: Result<u64, _> = (&param).try_into();
        assert!(result.is_err());
    }

    // TryFrom<&PathParam> for u32 测试
    #[test]
    fn test_try_from_path_param_to_u32_from_uint32() {
        let param = PathParam::UInt32(12345);
        let result: Result<u32, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 12345);
    }

    #[test]
    fn test_try_from_path_param_to_u32_from_int() {
        let param = PathParam::Int(42);
        let result: Result<u32, _> = (&param).try_into();
        assert!(result.is_err());
    }

    // TryFrom<&PathParam> for String 测试
    #[test]
    fn test_try_from_path_param_to_string_from_str() {
        let param = PathParam::Str(PathString::Owned("hello".to_string()));
        let result: Result<String, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_try_from_path_param_to_string_from_path() {
        let param = PathParam::Path(PathString::Owned("path/to/file".to_string()));
        let result: Result<String, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "path/to/file");
    }

    #[test]
    fn test_try_from_path_param_to_string_from_int() {
        let param = PathParam::Int(42);
        let result: Result<String, _> = (&param).try_into();
        assert!(result.is_err());
    }

    // TryFrom<&PathParam> for Uuid 测试
    #[test]
    fn test_try_from_path_param_to_uuid_from_uuid() {
        let uuid = Uuid::nil();
        let param = PathParam::Uuid(uuid);
        let result: Result<Uuid, _> = (&param).try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), uuid);
    }

    #[test]
    fn test_try_from_path_param_to_uuid_from_str() {
        let param = PathParam::Str(PathString::Owned("not-a-uuid".to_string()));
        let result: Result<Uuid, _> = (&param).try_into();
        assert!(result.is_err());
    }

    // PathString::borrowed 测试
    #[test]
    fn test_path_string_borrowed() {
        let source = Arc::from("/api/users/123");
        let range = 11..14; // "123"
        let path_string = PathString::borrowed(source, range);
        assert!(matches!(path_string, PathString::Borrowed(_)));
        assert_eq!(path_string.as_str(), "123");
    }

    // PathString::as_str 测试
    #[test]
    fn test_path_string_as_str_owned() {
        let path_string = PathString::Owned("hello world".to_string());
        assert_eq!(path_string.as_str(), "hello world");
    }

    #[test]
    fn test_path_string_as_str_borrowed() {
        let source = Arc::from("/test/path");
        let range = 1..5; // "test"
        let path_string = PathString::borrowed(source, range);
        assert_eq!(path_string.as_str(), "test");
    }

    // PathString::as_cow 测试
    #[test]
    fn test_path_string_as_cow_owned() {
        let path_string = PathString::Owned("hello".to_string());
        let cow = path_string.as_cow();
        assert!(matches!(cow, Cow::Borrowed(_)));
        assert_eq!(cow.as_ref(), "hello");
    }

    #[test]
    fn test_path_string_as_cow_borrowed() {
        let source = Arc::from("/api/v1/resource");
        let range = 5..7; // "v1"
        let path_string = PathString::borrowed(source, range);
        let cow = path_string.as_cow();
        assert!(matches!(cow, Cow::Borrowed(_)));
        assert_eq!(cow.as_ref(), "v1");
    }

    // PathSlice 测试
    #[test]
    fn test_path_slice_as_str() {
        let source = Arc::from("/users/123/posts");
        let range = 7..10; // "123"
        let slice = PathSlice { source, range };
        assert_eq!(slice.as_str(), "123");
    }

    #[test]
    fn test_path_slice_source() {
        let source: Arc<str> = Arc::from("/api/test");
        let range = 1..4; // "api"
        let slice = PathSlice {
            source: source.clone(),
            range,
        };
        assert_eq!(slice.source(), &source);
    }

    #[test]
    fn test_path_slice_range() {
        let source = Arc::from("/test/path");
        let range = 1..9;
        let slice = PathSlice {
            source,
            range: range.clone(),
        };
        assert_eq!(slice.range(), &range);
    }

    // PathParam Debug 测试
    #[test]
    fn test_path_param_debug_str() {
        let param = PathParam::Str(PathString::Owned("test".to_string()));
        let debug_str = format!("{:?}", param);
        assert!(debug_str.contains("Str"));
    }

    #[test]
    fn test_path_param_debug_int() {
        let param = PathParam::Int(42);
        let debug_str = format!("{:?}", param);
        assert!(debug_str.contains("Int"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_path_param_debug_uuid() {
        let uuid = Uuid::nil();
        let param = PathParam::Uuid(uuid);
        let debug_str = format!("{:?}", param);
        assert!(debug_str.contains("Uuid"));
    }

    // PathParam Clone 测试
    #[test]
    fn test_path_param_clone_str() {
        let param = PathParam::Str(PathString::Owned("original".to_string()));
        let cloned = param.clone();
        assert_eq!(param, cloned);
    }

    #[test]
    fn test_path_param_clone_int() {
        let param = PathParam::Int(123);
        let cloned = param.clone();
        assert_eq!(param, cloned);
    }

    // PathParam PartialEq 测试
    #[test]
    fn test_path_param_partial_eq_int() {
        let param1 = PathParam::Int(42);
        let param2 = PathParam::Int(42);
        let param3 = PathParam::Int(43);
        assert_eq!(param1, param2);
        assert_ne!(param1, param3);
    }

    #[test]
    fn test_path_param_partial_eq_str() {
        let param1 = PathParam::Str(PathString::Owned("test".to_string()));
        let param2 = PathParam::Str(PathString::Owned("test".to_string()));
        let param3 = PathParam::Str(PathString::Owned("other".to_string()));
        assert_eq!(param1, param2);
        assert_ne!(param1, param3);
    }

    // PathString Debug 测试
    #[test]
    fn test_path_string_debug_owned() {
        let path_string = PathString::Owned("test".to_string());
        let debug_str = format!("{:?}", path_string);
        assert!(debug_str.contains("Owned"));
    }

    #[test]
    fn test_path_string_debug_borrowed() {
        let source = Arc::from("/test");
        let range = 1..5;
        let path_string = PathString::borrowed(source, range);
        let debug_str = format!("{:?}", path_string);
        assert!(debug_str.contains("Borrowed"));
    }

    // PathString Clone 测试
    #[test]
    fn test_path_string_clone_owned() {
        let path_string = PathString::Owned("test".to_string());
        let cloned = path_string.clone();
        assert_eq!(path_string, cloned);
    }

    #[test]
    fn test_path_string_clone_borrowed() {
        let source: Arc<str> = Arc::from("/test");
        let range = 1..5;
        let path_string = PathString::borrowed(source.clone(), range.clone());
        let cloned = path_string.clone();
        assert_eq!(path_string, cloned);
    }

    // PathString PartialEq 测试
    #[test]
    fn test_path_string_partial_eq_owned() {
        let string1 = PathString::Owned("test".to_string());
        let string2 = PathString::Owned("test".to_string());
        let string3 = PathString::Owned("other".to_string());
        assert_eq!(string1, string2);
        assert_ne!(string1, string3);
    }

    #[test]
    fn test_path_string_partial_eq_borrowed() {
        let source: Arc<str> = Arc::from("/test/path");
        let string1 = PathString::borrowed(source.clone(), 1..5);
        let string2 = PathString::borrowed(source.clone(), 1..5);
        assert_eq!(string1, string2);
    }

    // PathSlice Debug 测试
    #[test]
    fn test_path_slice_debug() {
        let source = Arc::from("/test/path");
        let range = 1..9;
        let slice = PathSlice { source, range };
        let debug_str = format!("{:?}", slice);
        assert!(debug_str.contains("PathSlice"));
    }

    // PathSlice Clone 测试
    #[test]
    fn test_path_slice_clone() {
        let source = Arc::from("/test/path");
        let range = 1..9;
        let slice = PathSlice { source, range };
        let cloned = slice.clone();
        assert_eq!(slice.source(), cloned.source());
        assert_eq!(slice.range(), cloned.range());
    }

    // PathSlice PartialEq 测试
    #[test]
    fn test_path_slice_partial_eq() {
        let source: Arc<str> = Arc::from("/test/path");
        let range = 1..9;
        let slice1 = PathSlice {
            source: source.clone(),
            range: range.clone(),
        };
        let slice2 = PathSlice { source, range };
        assert_eq!(slice1, slice2);
    }

    // 边界条件测试
    #[test]
    fn test_path_param_zero_int() {
        let param: PathParam = 0i32.into();
        assert_eq!(param, PathParam::Int(0));
    }

    #[test]
    fn test_path_param_max_int() {
        let param: PathParam = i32::MAX.into();
        assert_eq!(param, PathParam::Int(i32::MAX));
    }

    #[test]
    fn test_path_param_min_int() {
        let param: PathParam = i32::MIN.into();
        assert_eq!(param, PathParam::Int(i32::MIN));
    }

    #[test]
    fn test_path_param_empty_string() {
        let s = String::new();
        let param: PathParam = s.into();
        assert!(matches!(param, PathParam::Str(PathString::Owned(_))));
        if let PathParam::Str(PathString::Owned(value)) = param {
            assert_eq!(value, "");
        }
    }

    #[test]
    fn test_path_string_empty_borrowed() {
        let source = Arc::from("/test");
        let range = 1..1; // 空范围
        let path_string = PathString::borrowed(source, range);
        assert_eq!(path_string.as_str(), "");
    }

    // Arc 共享测试
    #[test]
    fn test_arc_sharing_between_slices() {
        let source: Arc<str> = Arc::from("/shared/resource/123");
        let slice1 = PathSlice {
            source: source.clone(),
            range: 1..7, // "shared"
        };
        let slice2 = PathSlice {
            source: source.clone(),
            range: 16..19, // "123"
        };
        assert_eq!(slice1.source(), slice2.source());
        assert!(Arc::ptr_eq(slice1.source(), slice2.source()));
    }

    // Unicode 字符串测试
    #[test]
    fn test_path_param_unicode_string() {
        let s = String::from("测试文件🎉");
        let param: PathParam = s.clone().into();
        assert!(matches!(param, PathParam::Str(PathString::Owned(_))));
        if let PathParam::Str(PathString::Owned(value)) = param {
            assert_eq!(value, s);
        }
    }

    #[test]
    fn test_path_string_unicode_borrowed() {
        let source: Arc<str> = Arc::from("/测试/路径/123");
        let range = 1..7; // "测试" (6 bytes for 2 Chinese characters)
        let path_string = PathString::borrowed(source, range);
        assert_eq!(path_string.as_str(), "测试");
    }

    // 特殊字符测试
    #[test]
    fn test_path_param_special_chars() {
        let s = String::from("file-with_special.chars.txt");
        let param: PathParam = s.clone().into();
        if let PathParam::Str(PathString::Owned(value)) = param {
            assert_eq!(value, s);
        }
    }

    // Path vs Str 区别测试
    #[test]
    fn test_path_param_path_vs_str() {
        let source: Arc<str> = Arc::from("/files/path/to/resource");
        let param_str = PathParam::borrowed_str(source.clone(), 1..6); // "files"
        let param_path = PathParam::borrowed_path(source.clone(), 7..23); // "path/to/resource"

        assert!(matches!(param_str, PathParam::Str(_)));
        assert!(matches!(param_path, PathParam::Path(_)));
    }

    // PathString::Owned vs Borrowed 比较
    #[test]
    fn test_path_string_owned_vs_borrowed_same_content() {
        let source = Arc::from("/test");
        let borrowed = PathString::borrowed(source, 1..5);
        let owned = PathString::Owned("test".to_string());

        // 内容相同但类型不同，所以不相等
        assert_ne!(borrowed, owned);
        // 但 as_str() 结果相同
        assert_eq!(borrowed.as_str(), owned.as_str());
    }

    // 大数值测试
    #[test]
    fn test_path_param_large_u64() {
        let value: u64 = u64::MAX;
        let param: PathParam = value.into();
        assert_eq!(param, PathParam::UInt64(u64::MAX));
    }

    #[test]
    fn test_path_param_large_i64() {
        let value: i64 = i64::MAX;
        let param: PathParam = value.into();
        assert_eq!(param, PathParam::Int64(i64::MAX));
    }

    // TryFrom 错误类型验证
    #[test]
    fn test_try_from_error_type() {
        let param = PathParam::Str(PathString::Owned("test".to_string()));
        let result: Result<i32, _> = (&param).try_into();
        assert!(result.is_err());
        if let Err(e) = result {
            // 验证错误类型是 SilentError::ParamsNotFound
            assert!(matches!(e, SilentError::ParamsNotFound));
        }
    }

    // PathSlice range 边界测试
    #[test]
    fn test_path_slice_range_at_end() {
        let source = Arc::from("/test/path/123");
        let range = 11..14; // "123"
        let slice = PathSlice { source, range };
        assert_eq!(slice.as_str(), "123");
    }

    #[test]
    fn test_path_slice_range_at_start() {
        let source = Arc::from("/test/path");
        let range = 0..1; // "/"
        let slice = PathSlice { source, range };
        assert_eq!(slice.as_str(), "/");
    }

    // 多个 PathParam 实例比较
    #[test]
    fn test_multiple_path_params_comparison() {
        let params = [
            PathParam::Int(1),
            PathParam::Str(PathString::Owned("test".to_string())),
            PathParam::Int64(100),
            PathParam::Uuid(Uuid::nil()),
        ];

        assert_eq!(params[0], PathParam::Int(1));
        assert_eq!(
            params[1],
            PathParam::Str(PathString::Owned("test".to_string()))
        );
    }
}
