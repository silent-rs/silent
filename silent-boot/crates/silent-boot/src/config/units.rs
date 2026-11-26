//! 配置里带单位的两类数值：时长和字节数。
//!
//! 两者都既能从 `config/app.toml` 里读，也能从环境变量里读，写法相同：
//!
//! | 类型 | 带单位 | 不带单位 |
//! |---|---|---|
//! | [`Duration`] | `"30s"`、`"500ms"`、`"1m 30s"`、`"2h"`、`"7d"` | 整数，单位是秒 |
//! | [`ByteSize`] | `"10MiB"`、`"10MB"`、`"1GiB"` | 整数，单位是字节 |

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// 时长，解析规则见模块说明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(pub std::time::Duration);

impl Duration {
    /// 以秒计的时长。
    pub const fn from_secs(secs: u64) -> Self {
        Self(std::time::Duration::from_secs(secs))
    }

    /// 以毫秒计的时长。
    pub const fn from_millis(millis: u64) -> Self {
        Self(std::time::Duration::from_millis(millis))
    }

    /// 解析 `"30s"`、`"500ms"`、`"1m 30s"` 这样的写法。
    pub fn parse(text: &str) -> Result<Self, ParseUnitError> {
        let mut total = std::time::Duration::ZERO;
        let mut tokens = 0;
        for token in text.split_whitespace() {
            let (value, unit) = split_number(token, text)?;
            let nanos = match unit.as_str() {
                "" | "s" => 1_000_000_000,
                "ms" => 1_000_000,
                "us" | "µs" => 1_000,
                "ns" => 1,
                "m" => 60_000_000_000,
                "h" => 3_600_000_000_000,
                "d" => 86_400_000_000_000,
                _ => return Err(ParseUnitError::new(text)),
            };
            total += std::time::Duration::from_nanos(value.saturating_mul(nanos));
            tokens += 1;
        }
        if tokens == 0 {
            return Err(ParseUnitError::new(text));
        }
        Ok(Self(total))
    }
}

impl Default for Duration {
    fn default() -> Self {
        Self(std::time::Duration::ZERO)
    }
}

impl From<std::time::Duration> for Duration {
    fn from(value: std::time::Duration) -> Self {
        Self(value)
    }
}

impl From<Duration> for std::time::Duration {
    fn from(value: Duration) -> Self {
        value.0
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0.as_secs_f64())
    }
}

impl Serialize for Duration {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = Duration;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a duration such as \"30s\", \"500ms\" or a number of seconds")
            }

            fn visit_u64<E: de::Error>(self, secs: u64) -> Result<Duration, E> {
                Ok(Duration::from_secs(secs))
            }

            fn visit_i64<E: de::Error>(self, secs: i64) -> Result<Duration, E> {
                u64::try_from(secs)
                    .map(Duration::from_secs)
                    .map_err(|_| E::custom("duration cannot be negative"))
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Duration, E> {
                Duration::parse(text).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// 字节数，解析规则见模块说明。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(pub u64);

impl ByteSize {
    /// 以字节计的大小。
    pub const fn bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// 以 MiB 计的大小。
    pub const fn mib(mib: u64) -> Self {
        Self(mib << 20)
    }

    /// 解析 `"10MiB"`、`"10MB"` 这样的写法。不带 `i` 的单位按 1000 进，带 `i` 的按 1024 进。
    pub fn parse(text: &str) -> Result<Self, ParseUnitError> {
        let (value, unit) = split_number(text.trim(), text)?;
        let multiplier = match unit.to_ascii_uppercase().as_str() {
            "" | "B" => 1,
            "KB" => 1_000,
            "MB" => 1_000_000,
            "GB" => 1_000_000_000,
            "KIB" => 1 << 10,
            "MIB" => 1 << 20,
            "GIB" => 1 << 30,
            _ => return Err(ParseUnitError::new(text)),
        };
        value
            .checked_mul(multiplier)
            .map(Self)
            .ok_or_else(|| ParseUnitError::new(text))
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ByteSize {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ByteSize {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = ByteSize;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a size such as \"10MiB\" or a number of bytes")
            }

            fn visit_u64<E: de::Error>(self, bytes: u64) -> Result<ByteSize, E> {
                Ok(ByteSize(bytes))
            }

            fn visit_i64<E: de::Error>(self, bytes: i64) -> Result<ByteSize, E> {
                u64::try_from(bytes)
                    .map(ByteSize)
                    .map_err(|_| E::custom("size cannot be negative"))
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<ByteSize, E> {
                ByteSize::parse(text).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// 把 `"500ms"` 拆成数值 `500` 和单位 `ms`。没有单位时单位是空串。
fn split_number(token: &str, whole: &str) -> Result<(u64, String), ParseUnitError> {
    let unit_at = token
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(token.len());
    let value = token[..unit_at]
        .parse()
        .map_err(|_| ParseUnitError::new(whole))?;
    Ok((value, token[unit_at..].to_owned()))
}

/// 时长或字节数的写法不对。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "无法识别 {text:?}：时长写成 \"30s\"、\"500ms\"、\"1m 30s\"，大小写成 \"10MiB\"，或者直接写数字"
)]
pub struct ParseUnitError {
    text: String,
}

impl ParseUnitError {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_accepts_units_and_bare_seconds() {
        assert_eq!(Duration::parse("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(Duration::parse("500ms"), Ok(Duration::from_millis(500)));
        assert_eq!(Duration::parse("1m 30s"), Ok(Duration::from_secs(90)));
        assert_eq!(Duration::parse("2h"), Ok(Duration::from_secs(7_200)));
        assert_eq!(Duration::parse("7d"), Ok(Duration::from_secs(604_800)));
        assert_eq!(Duration::parse("45"), Ok(Duration::from_secs(45)));
    }

    #[test]
    fn duration_rejects_garbage() {
        assert!(Duration::parse("").is_err());
        assert!(Duration::parse("soon").is_err());
        assert!(Duration::parse("1y").is_err());
        assert!(Duration::parse("-5s").is_err());
    }

    #[test]
    fn byte_size_accepts_decimal_and_binary_units() {
        assert_eq!(ByteSize::parse("10MiB"), Ok(ByteSize::mib(10)));
        assert_eq!(ByteSize::parse("10MB"), Ok(ByteSize(10_000_000)));
        assert_eq!(ByteSize::parse("1GiB"), Ok(ByteSize(1 << 30)));
        assert_eq!(ByteSize::parse("1024"), Ok(ByteSize(1024)));
        assert!(ByteSize::parse("10XB").is_err());
    }

    #[test]
    fn units_deserialize_from_toml_and_env_style_values() {
        #[derive(Default, Deserialize)]
        struct Holder {
            a: Duration,
            b: ByteSize,
        }
        let holder: Holder = toml::from_str("a = \"1m 30s\"\nb = \"10MiB\"").unwrap();
        assert_eq!(holder.a, Duration::from_secs(90));
        assert_eq!(holder.b, ByteSize::mib(10));

        let holder: Holder = toml::from_str("a = 12\nb = 2048").unwrap();
        assert_eq!(holder.a, Duration::from_secs(12));
        assert_eq!(holder.b, ByteSize(2048));
    }
}
