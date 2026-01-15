use super::{CowValue, ValError, ValueEnumAccess};
use serde::de::value::SeqDeserializer;
use serde::de::{Deserialize, Deserializer, Error as DeError, IntoDeserializer, Visitor};
use serde::forward_to_deserialize_any;
use std::borrow::Cow;
use std::iter::Iterator;

pub(crate) fn from_str_multi_val<'de, I, T, C>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = C> + 'de,
    T: Deserialize<'de>,
    C: Into<Cow<'de, str>> + Eq + 'de,
{
    let iter = input.into_iter().map(|v| CowValue(v.into()));
    T::deserialize(VecValue(iter))
}

macro_rules! forward_vec_parsed_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where V: Visitor<'de>
            {
                if let Some(item) = self.0.into_iter().next() {
                    match item.0.parse::<$ty>() {
                        Ok(val) => val.into_deserializer().$method(visitor),
                        Err(e) => Err(DeError::custom(e))
                    }
                } else {
                    Err(DeError::custom("expected vec not empty"))
                }
            }
        )*
    }
}

struct VecValue<I>(I);

impl<'de, I> IntoDeserializer<'de> for VecValue<I>
where
    I: Iterator<Item = CowValue<'de>>,
{
    type Deserializer = Self;

    #[inline]
    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de, I> Deserializer<'de> for VecValue<I>
where
    I: IntoIterator<Item = CowValue<'de>>,
{
    type Error = ValError;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0.into_iter().next() {
            Some(item) => item.deserialize_any(visitor),
            _ => Err(DeError::custom("expected vec not empty")),
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
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(SeqDeserializer::new(self.0.into_iter()))
    }

    #[inline]
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }
    #[inline]
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
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
        match self.0.into_iter().next() {
            Some(item) => visitor.visit_enum(ValueEnumAccess(item.0)),
            _ => Err(DeError::custom("expected vec not empty")),
        }
    }

    forward_to_deserialize_any! {
        char
        str
        string
        unit
        bytes
        byte_buf
        unit_struct
        struct
        identifier
        ignored_any
        map
    }

    forward_vec_parsed_value! {
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

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_from_str_multi_val_single_string() {
        let input = vec!["hello"];
        let result: String = from_str_multi_val(input).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_from_str_multi_val_vec_string() {
        let input = vec!["hello", "world", "test"];
        let result: Vec<String> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec!["hello", "world", "test"]);
    }

    #[test]
    fn test_from_str_multi_val_vec_i32() {
        let input = vec!["1", "2", "3"];
        let result: Vec<i32> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_from_str_multi_val_vec_f64() {
        let input = vec!["1.5", "2.7", "3.0"];
        let result: Vec<f64> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![1.5, 2.7, 3.0]);
    }

    #[test]
    fn test_from_str_multi_val_vec_bool() {
        let input = vec!["true", "false"];
        let result: Vec<bool> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![true, false]);
    }

    #[test]
    fn test_from_str_multi_val_empty_vec() {
        let input: Vec<&str> = vec![];
        let result: Vec<String> = from_str_multi_val(input).unwrap();
        assert_eq!(result, Vec::<String>::new());
    }

    // ==================== 数值类型测试 ====================

    #[test]
    fn test_from_str_multi_val_u8() {
        let input = vec!["255"];
        let result: u8 = from_str_multi_val(input).unwrap();
        assert_eq!(result, 255);
    }

    #[test]
    fn test_from_str_multi_val_u16() {
        let input = vec!["65535"];
        let result: u16 = from_str_multi_val(input).unwrap();
        assert_eq!(result, 65535);
    }

    #[test]
    fn test_from_str_multi_val_u32() {
        let input = vec!["4294967295"];
        let result: u32 = from_str_multi_val(input).unwrap();
        assert_eq!(result, 4294967295);
    }

    #[test]
    fn test_from_str_multi_val_u64() {
        let input = vec!["18446744073709551615"];
        let result: u64 = from_str_multi_val(input).unwrap();
        assert_eq!(result, 18446744073709551615);
    }

    #[test]
    fn test_from_str_multi_val_i8() {
        let input = vec!["-128"];
        let result: i8 = from_str_multi_val(input).unwrap();
        assert_eq!(result, -128);
    }

    #[test]
    fn test_from_str_multi_val_i16() {
        let input = vec!["-32768"];
        let result: i16 = from_str_multi_val(input).unwrap();
        assert_eq!(result, -32768);
    }

    #[test]
    fn test_from_str_multi_val_i32() {
        let input = vec!["-2147483648"];
        let result: i32 = from_str_multi_val(input).unwrap();
        assert_eq!(result, -2147483648);
    }

    #[test]
    fn test_from_str_multi_val_i64() {
        let input = vec!["-9223372036854775808"];
        let result: i64 = from_str_multi_val(input).unwrap();
        assert_eq!(result, -9223372036854775808);
    }

    #[test]
    fn test_from_str_multi_val_f32() {
        let input = vec!["2.5"];
        let result: f32 = from_str_multi_val(input).unwrap();
        assert!((result - 2.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_from_str_multi_val_f64() {
        let input = vec!["1.414"];
        let result: f64 = from_str_multi_val(input).unwrap();
        assert!((result - 1.414).abs() < f64::EPSILON);
    }

    // ==================== Option 类型测试 ====================

    #[test]
    fn test_from_str_multi_val_option_some() {
        let input = vec!["hello"];
        let result: Option<String> = from_str_multi_val(input).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_from_str_multi_val_option_vec() {
        let input = vec!["a", "b", "c"];
        let result: Option<Vec<String>> = from_str_multi_val(input).unwrap();
        assert_eq!(
            result,
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    // ==================== 元组测试 ====================

    #[test]
    fn test_from_str_multi_val_tuple() {
        let input = vec!["hello", "world"];
        let result: (String, String) = from_str_multi_val(input).unwrap();
        assert_eq!(result, ("hello".to_string(), "world".to_string()));
    }

    #[test]
    fn test_from_str_multi_val_tuple_three() {
        let input = vec!["1", "2", "3"];
        let result: (i32, i32, i32) = from_str_multi_val(input).unwrap();
        assert_eq!(result, (1, 2, 3));
    }

    #[test]
    fn test_from_str_multi_val_tuple_mixed_types() {
        let input = vec!["42", "hello", "2.5"];
        let result: (i32, String, f64) = from_str_multi_val(input).unwrap();
        assert_eq!(result, (42, "hello".to_string(), 2.5));
    }

    // ==================== 枚举测试 ====================

    #[derive(Deserialize, Debug, PartialEq)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    #[test]
    fn test_from_str_multi_val_enum_unit_variant() {
        let input = vec!["Active"];
        let result: Status = from_str_multi_val(input).unwrap();
        assert_eq!(result, Status::Active);
    }

    #[derive(Deserialize, Debug, PartialEq)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    #[test]
    fn test_from_str_multi_val_enum_different_variants() {
        let result1: Color = from_str_multi_val(vec!["Red"]).unwrap();
        assert_eq!(result1, Color::Red);

        let result2: Color = from_str_multi_val(vec!["Green"]).unwrap();
        assert_eq!(result2, Color::Green);

        let result3: Color = from_str_multi_val(vec!["Blue"]).unwrap();
        assert_eq!(result3, Color::Blue);
    }

    // ==================== 错误处理测试 ====================

    #[test]
    fn test_from_str_multi_val_invalid_number() {
        let input = vec!["not_a_number"];
        let result: Result<i32, _> = from_str_multi_val(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_multi_val_invalid_bool() {
        let input = vec!["not_true_or_false"];
        let result: Result<bool, _> = from_str_multi_val(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_multi_val_overflow_u8() {
        let input = vec!["256"];
        let result: Result<u8, _> = from_str_multi_val(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_multi_val_overflow_i8() {
        let input = vec!["129"];
        let result: Result<i8, _> = from_str_multi_val(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_multi_vec_single_bool() {
        let input = vec!["true"];
        let result: Vec<bool> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![true]);
    }

    #[test]
    fn test_from_str_multi_val_multiple_bool() {
        let input = vec!["true", "false", "true"];
        let result: Vec<bool> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![true, false, true]);
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_from_str_multi_val_single_char_string() {
        let input = vec!["a"];
        let result: String = from_str_multi_val(input).unwrap();
        assert_eq!(result, "a");
    }

    #[test]
    fn test_from_str_multi_val_long_string() {
        let input =
            vec!["this is a very long string with spaces and special characters !@#$%^&*()"];
        let result: String = from_str_multi_val(input).unwrap();
        assert_eq!(
            result,
            "this is a very long string with spaces and special characters !@#$%^&*()"
        );
    }

    #[test]
    fn test_from_str_multi_val_zero() {
        let input = vec!["0"];
        let result: i32 = from_str_multi_val(input).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_from_str_multi_val_negative_zero() {
        let input = vec!["-0"];
        let result: i32 = from_str_multi_val(input).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_from_str_multi_val_vec_negative_numbers() {
        let input = vec!["-1", "-2", "-3"];
        let result: Vec<i32> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![-1, -2, -3]);
    }

    #[test]
    fn test_from_str_multi_val_mixed_positive_negative() {
        let input = vec!["-1", "0", "1"];
        let result: Vec<i32> = from_str_multi_val(input).unwrap();
        assert_eq!(result, vec![-1, 0, 1]);
    }

    #[test]
    fn test_from_str_multi_val_scientific_notation() {
        let input = vec!["1.23e2"];
        let result: f64 = from_str_multi_val(input).unwrap();
        assert!((result - 123.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_from_str_multi_very_large_vec() {
        let input: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        let result: Vec<i32> = from_str_multi_val(input).unwrap();
        assert_eq!(result.len(), 100);
        assert_eq!(result[0], 0);
        assert_eq!(result[99], 99);
    }
}
