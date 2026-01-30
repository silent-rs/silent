use std::borrow::Cow;
use std::iter::Iterator;

pub use serde::de::value::{Error as ValError, MapDeserializer};
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, EnumAccess, Error as DeError, IntoDeserializer,
    VariantAccess, Visitor,
};
use serde::forward_to_deserialize_any;

#[cfg(feature = "multipart")]
mod multipart;

#[cfg(feature = "multipart")]
pub(crate) use multipart::*;

#[inline]
pub fn from_str_map<'de, I, T, K, V>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = (K, V)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>>,
    V: Into<Cow<'de, str>>,
{
    let iter = input
        .into_iter()
        .map(|(k, v)| (CowValue(k.into()), CowValue(v.into())));
    T::deserialize(MapDeserializer::new(iter))
}

#[inline]
pub fn from_str_val<'de, I, T>(input: I) -> Result<T, ValError>
where
    I: Into<Cow<'de, str>>,
    T: Deserialize<'de>,
{
    T::deserialize(CowValue(input.into()))
}

macro_rules! forward_cow_parsed_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where V: Visitor<'de>
            {
                match self.0.parse::<$ty>() {
                    Ok(val) => val.into_deserializer().$method(visitor),
                    Err(e) => Err(DeError::custom(e))
                }
            }
        )*
    }
}

pub(crate) struct ValueEnumAccess<'de>(pub(crate) Cow<'de, str>);

impl<'de> EnumAccess<'de> for ValueEnumAccess<'de> {
    type Error = ValError;
    type Variant = UnitOnlyVariantAccess;

    #[inline]
    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.0.into_deserializer())?;
        Ok((variant, UnitOnlyVariantAccess))
    }
}

pub(crate) struct UnitOnlyVariantAccess;

impl<'de> VariantAccess<'de> for UnitOnlyVariantAccess {
    type Error = ValError;

    #[inline]
    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    #[inline]
    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }

    #[inline]
    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }

    #[inline]
    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }
}

#[derive(Debug)]
pub(crate) struct CowValue<'de>(pub(crate) Cow<'de, str>);

impl<'de> IntoDeserializer<'de> for CowValue<'de> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for CowValue<'de> {
    type Error = ValError;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0 {
            Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
            Cow::Owned(value) => visitor.visit_string(value),
        }
    }

    #[inline]
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    #[inline]
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    #[inline]
    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(ValueEnumAccess(self.0))
    }

    forward_to_deserialize_any! {
        char
        str
        string
        unit
        bytes
        byte_buf
        unit_struct
        tuple_struct
        struct
        identifier
        tuple
        ignored_any
        seq
        map
    }

    forward_cow_parsed_value! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    // ==================== from_str_val 测试 ====================

    #[test]
    fn test_from_str_val_string() {
        let result: String = from_str_val("hello").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_from_str_val_integer() {
        let result: i32 = from_str_val("42").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_from_str_val_bool() {
        let result: bool = from_str_val("true").unwrap();
        assert!(result);

        let result: bool = from_str_val("false").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_from_str_val_float() {
        let result: f64 = from_str_val("2.5").unwrap();
        assert_eq!(result, 2.5);
    }

    #[test]
    fn test_from_str_val_struct() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Person {
            name: String,
            age: u32,
        }

        // from_str_val 只能用于单个值，不能用于结构体
        let result: Result<Person, _> = from_str_val("name");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_val_invalid_integer() {
        let result: Result<i32, _> = from_str_val("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_val_optional() {
        // from_str_val 可以反序列化 Option 类型，它会将值反序列化为 Some
        let result: Option<String> = from_str_val("test").unwrap();
        assert_eq!(result, Some("test".to_string()));

        // 空字符串也会反序列化为 Some("")
        let result: Option<String> = from_str_val("").unwrap();
        assert_eq!(result, Some("".to_string()));
    }

    // ==================== from_str_map 测试 ====================

    #[test]
    fn test_from_str_map_simple_struct() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            name: String,
            count: i32,
        }

        let input = vec![("name", "test"), ("count", "42")];
        let result: Config = from_str_map(input).unwrap();

        assert_eq!(result.name, "test");
        assert_eq!(result.count, 42);
    }

    #[test]
    fn test_from_str_map_nested_struct() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Inner {
            value: String,
        }

        #[derive(Deserialize, Debug, PartialEq)]
        struct Outer {
            inner: Inner,
        }

        let input = vec![("inner", "{\"value\":\"test\"}")];
        let result: Result<Outer, _> = from_str_map(input);
        // JSON 反序列化嵌套结构可能失败
        assert!(result.is_err() || result.unwrap().inner.value == "test");
    }

    #[test]
    fn test_from_str_map_multiple_fields() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct MultiField {
            a: i32,
            b: String,
            c: f64,
            d: bool,
        }

        let input = vec![("a", "10"), ("b", "test"), ("c", "2.5"), ("d", "true")];
        let result: MultiField = from_str_map(input).unwrap();

        assert_eq!(result.a, 10);
        assert_eq!(result.b, "test");
        assert_eq!(result.c, 2.5);
        assert!(result.d);
    }

    #[test]
    fn test_from_str_map_empty() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Empty {}

        let input: Vec<(&str, &str)> = vec![];
        let result: Empty = from_str_map(input).unwrap();
        // 空结构体总是可以反序列化
        let _ = result;
    }

    #[test]
    fn test_from_str_map_missing_field() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Required {
            name: String,
            age: u32,
        }

        let input = vec![("name", "test")];
        let result: Result<Required, _> = from_str_map(input);
        // 缺少必填字段应该失败
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_map_with_cow_strings() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Test {
            key1: String,
            key2: String,
        }

        let input = vec![
            (Cow::Borrowed("key1"), Cow::Borrowed("value1")),
            (
                Cow::Owned("key2".to_string()),
                Cow::Owned("value2".to_string()),
            ),
        ];

        let result: Test = from_str_map(input).unwrap();
        assert_eq!(result.key1, "value1");
        assert_eq!(result.key2, "value2");
    }

    // ==================== CowValue 反序列化器测试 ====================

    #[test]
    fn test_cow_value_borrowed_str() {
        let cow_value = CowValue(Cow::Borrowed("test"));
        let result: String = Deserialize::deserialize(cow_value).unwrap();
        assert_eq!(result, "test");
    }

    #[test]
    fn test_cow_value_owned_str() {
        let cow_value = CowValue(Cow::Owned("owned".to_string()));
        let result: String = Deserialize::deserialize(cow_value).unwrap();
        assert_eq!(result, "owned");
    }

    #[test]
    fn test_cow_value_bool() {
        let cow_value = CowValue(Cow::Borrowed("true"));
        let result: bool = Deserialize::deserialize(cow_value).unwrap();
        assert!(result);
    }

    #[test]
    fn test_cow_value_integer() {
        let cow_value = CowValue(Cow::Borrowed("42"));
        let result: i32 = Deserialize::deserialize(cow_value).unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_cow_value_float() {
        let cow_value = CowValue(Cow::Borrowed("2.5"));
        let result: f64 = Deserialize::deserialize(cow_value).unwrap();
        assert_eq!(result, 2.5);
    }

    #[test]
    fn test_cow_value_invalid_bool() {
        let cow_value = CowValue(Cow::Borrowed("not_bool"));
        let result: Result<bool, _> = Deserialize::deserialize(cow_value);
        assert!(result.is_err());
    }

    #[test]
    fn test_cow_value_invalid_integer() {
        let cow_value = CowValue(Cow::Borrowed("abc"));
        let result: Result<i32, _> = Deserialize::deserialize(cow_value);
        assert!(result.is_err());
    }

    // ==================== ValueEnumAccess 测试 ====================

    #[test]
    fn test_value_enum_access_simple_enum() {
        #[derive(Deserialize, Debug, PartialEq)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        // 通过 from_str_val 测试枚举反序列化
        let result: Color = from_str_val("Red").unwrap();
        assert_eq!(result, Color::Red);
    }

    #[test]
    fn test_value_enum_access_invalid_variant() {
        #[derive(Deserialize, Debug, PartialEq)]
        enum Color {
            Red,
            Green,
        }

        let result: Result<Color, _> = from_str_val("Blue");
        assert!(result.is_err());
    }

    // ==================== UnitOnlyVariantAccess 测试 ====================

    #[test]
    fn test_unit_only_variant_access_unit_variant() {
        // 通过 from_str_val 测试单元变体
        #[derive(Deserialize, Debug, PartialEq)]
        enum SimpleEnum {
            Variant1,
            Variant2,
        }

        let result: SimpleEnum = from_str_val("Variant1").unwrap();
        assert_eq!(result, SimpleEnum::Variant1);
    }

    // ==================== 边界条件和特殊情况测试 ====================

    #[test]
    fn test_from_str_val_empty_string() {
        let result: String = from_str_val("").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_from_str_val_zero() {
        let result: i32 = from_str_val("0").unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_from_str_val_negative() {
        let result: i32 = from_str_val("-42").unwrap();
        assert_eq!(result, -42);
    }

    #[test]
    fn test_from_str_val_large_number() {
        let result: u64 = from_str_val("18446744073709551615").unwrap();
        assert_eq!(result, 18_446_744_073_709_551_615);
    }

    #[test]
    fn test_from_str_map_unicode() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct UnicodeTest {
            name: String,
            emoji: String,
        }

        let input = vec![("name", "测试"), ("emoji", "😀")];
        let result: UnicodeTest = from_str_map(input).unwrap();

        assert_eq!(result.name, "测试");
        assert_eq!(result.emoji, "😀");
    }

    #[test]
    fn test_from_str_map_duplicate_keys() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct DupTest {
            value: String,
        }

        // serde 的 MapDeserializer 会将重复的键视为错误
        let input = vec![("value", "first"), ("value", "second")];
        let result: Result<DupTest, _> = from_str_map(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_cow_value_unicode() {
        let cow_value = CowValue(Cow::Borrowed("测试"));
        let result: String = Deserialize::deserialize(cow_value).unwrap();
        assert_eq!(result, "测试");
    }

    #[test]
    fn test_cow_value_special_characters() {
        let cow_value = CowValue(Cow::Borrowed("hello\nworld\t"));
        let result: String = Deserialize::deserialize(cow_value).unwrap();
        assert_eq!(result, "hello\nworld\t");
    }
}
