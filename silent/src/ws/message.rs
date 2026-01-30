use crate::{Result, SilentError};
use async_tungstenite::tungstenite::protocol;
use std::fmt;
use std::fmt::Formatter;
use std::ops::Deref;

#[derive(Eq, PartialEq, Clone)]
pub struct Message {
    pub(crate) inner: protocol::Message,
}

impl Deref for Message {
    type Target = protocol::Message;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Message {
    /// Construct a new Text `Message`.
    #[inline]
    pub fn text<S: Into<String>>(s: S) -> Message {
        Message {
            inner: protocol::Message::Text(s.into().into()),
        }
    }

    /// Construct a new Binary `Message`.
    #[inline]
    pub fn binary<V: Into<Vec<u8>>>(v: V) -> Message {
        Message {
            inner: protocol::Message::Binary(v.into().into()),
        }
    }

    /// Construct a new Ping `Message`.
    #[inline]
    pub fn ping<V: Into<Vec<u8>>>(v: V) -> Message {
        Message {
            inner: protocol::Message::Ping(v.into().into()),
        }
    }

    /// Construct a new pong `Message`.
    #[inline]
    pub fn pong<V: Into<Vec<u8>>>(v: V) -> Message {
        Message {
            inner: protocol::Message::Pong(v.into().into()),
        }
    }

    /// Construct the default Close `Message`.
    #[inline]
    pub fn close() -> Message {
        Message {
            inner: protocol::Message::Close(None),
        }
    }

    /// Construct a Close `Message` with a code and reason.
    #[inline]
    pub fn close_with(code: impl Into<u16>, reason: impl Into<String>) -> Message {
        Message {
            inner: protocol::Message::Close(Some(protocol::frame::CloseFrame {
                code: protocol::frame::coding::CloseCode::from(code.into()),
                reason: reason.into().into(),
            })),
        }
    }

    /// Returns true if this message is a Text message.
    #[inline]
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    /// Returns true if this message is a Binary message.
    #[inline]
    pub fn is_binary(&self) -> bool {
        self.inner.is_binary()
    }

    /// Returns true if this message is a Close message.
    #[inline]
    pub fn is_close(&self) -> bool {
        self.inner.is_close()
    }

    /// Returns true if this message is a Ping message.
    #[inline]
    pub fn is_ping(&self) -> bool {
        self.inner.is_ping()
    }

    /// Returns true if this message is a Pong message.
    #[inline]
    pub fn is_pong(&self) -> bool {
        self.inner.is_pong()
    }

    /// Try to get the close frame (close code and reason).
    #[inline]
    pub fn close_frame(&self) -> Option<(u16, &str)> {
        if let protocol::Message::Close(Some(ref close_frame)) = self.inner {
            Some((close_frame.code.into(), close_frame.reason.as_ref()))
        } else {
            None
        }
    }

    /// Try to get a reference to the string text, if this is a Text message.
    #[inline]
    pub fn to_str(&self) -> Result<&str> {
        self.inner
            .to_text()
            .map_err(|_| SilentError::WsError("not a text message".into()))
    }

    /// Returns the bytes of this message, if the message can contain data.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match self.inner {
            protocol::Message::Text(ref s) => s.as_bytes(),
            protocol::Message::Binary(ref v) => v.as_ref(),
            protocol::Message::Ping(ref v) => v.as_ref(),
            protocol::Message::Pong(ref v) => v.as_ref(),
            protocol::Message::Close(_) => &[],
            protocol::Message::Frame(ref v) => v.payload(),
        }
    }

    /// Destructure this message into binary data.
    #[inline]
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.into_data().to_vec()
    }
}

impl fmt::Debug for Message {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_message_text() {
        let msg = Message::text("hello");
        assert!(msg.is_text());
        assert!(!msg.is_binary());
        assert!(!msg.is_close());
        assert!(!msg.is_ping());
        assert!(!msg.is_pong());
    }

    #[test]
    fn test_message_binary() {
        let msg = Message::binary(vec![1, 2, 3, 4]);
        assert!(msg.is_binary());
        assert!(!msg.is_text());
        assert!(!msg.is_close());
        assert!(!msg.is_ping());
        assert!(!msg.is_pong());
    }

    #[test]
    fn test_message_ping() {
        let msg = Message::ping(vec![1, 2, 3]);
        assert!(msg.is_ping());
        assert!(!msg.is_pong());
        assert!(!msg.is_text());
        assert!(!msg.is_binary());
        assert!(!msg.is_close());
    }

    #[test]
    fn test_message_pong() {
        let msg = Message::pong(vec![1, 2, 3]);
        assert!(msg.is_pong());
        assert!(!msg.is_ping());
        assert!(!msg.is_text());
        assert!(!msg.is_binary());
        assert!(!msg.is_close());
    }

    #[test]
    fn test_message_close() {
        let msg = Message::close();
        assert!(msg.is_close());
        assert!(!msg.is_text());
        assert!(!msg.is_binary());
        assert!(!msg.is_ping());
        assert!(!msg.is_pong());
    }

    #[test]
    fn test_message_close_with_code_and_reason() {
        let msg = Message::close_with(1000u16, "Normal closure");
        assert!(msg.is_close());
        let frame = msg.close_frame();
        assert_eq!(frame, Some((1000, "Normal closure")));
    }

    // ==================== 类型检查测试 ====================

    #[test]
    fn test_message_is_text() {
        let msg = Message::text("test");
        assert!(msg.is_text());
    }

    #[test]
    fn test_message_is_binary() {
        let msg = Message::binary(vec![0x00, 0x01]);
        assert!(msg.is_binary());
    }

    #[test]
    fn test_message_is_ping() {
        let msg = Message::ping(b"ping");
        assert!(msg.is_ping());
    }

    #[test]
    fn test_message_is_pong() {
        let msg = Message::pong(b"pong");
        assert!(msg.is_pong());
    }

    #[test]
    fn test_message_is_close() {
        let msg = Message::close();
        assert!(msg.is_close());
    }

    // ==================== 内容提取测试 ====================

    #[test]
    fn test_message_to_str() {
        let msg = Message::text("hello world");
        assert_eq!(msg.to_str().unwrap(), "hello world");
    }

    #[test]
    fn test_message_to_str_binary_fails() {
        // 使用无效的 UTF-8 字节序列
        let msg = Message::binary(vec![0xFF, 0xFE]);
        assert!(msg.to_str().is_err());
    }

    #[test]
    fn test_message_as_bytes_text() {
        let msg = Message::text("hello");
        assert_eq!(msg.as_bytes(), b"hello");
    }

    #[test]
    fn test_message_as_bytes_binary() {
        let data = vec![1u8, 2, 3, 4];
        let msg = Message::binary(data.clone());
        assert_eq!(msg.as_bytes(), data.as_slice());
    }

    #[test]
    fn test_message_as_bytes_ping() {
        let data = vec![1u8, 2, 3];
        let msg = Message::ping(data.clone());
        assert_eq!(msg.as_bytes(), data.as_slice());
    }

    #[test]
    fn test_message_as_bytes_pong() {
        let data = vec![1u8, 2, 3];
        let msg = Message::pong(data.clone());
        assert_eq!(msg.as_bytes(), data.as_slice());
    }

    #[test]
    fn test_message_as_bytes_close() {
        let msg = Message::close();
        assert_eq!(msg.as_bytes(), b"" as &[u8]);
    }

    #[test]
    fn test_message_into_bytes_text() {
        let msg = Message::text("hello");
        assert_eq!(msg.into_bytes(), b"hello".to_vec());
    }

    #[test]
    fn test_message_into_bytes_binary() {
        let data = vec![1u8, 2, 3, 4];
        let msg = Message::binary(data.clone());
        assert_eq!(msg.into_bytes(), data);
    }

    // ==================== Close Frame 测试 ====================

    #[test]
    fn test_message_close_frame_none() {
        let msg = Message::close();
        assert!(msg.close_frame().is_none());
    }

    #[test]
    fn test_message_close_frame_some() {
        let msg = Message::close_with(1000u16, "Normal closure");
        let frame = msg.close_frame();
        assert_eq!(frame, Some((1000, "Normal closure")));
    }

    #[test]
    fn test_message_close_frame_custom_code() {
        let msg = Message::close_with(4000u16, "Custom reason");
        let frame = msg.close_frame();
        assert_eq!(frame, Some((4000, "Custom reason")));
    }

    #[test]
    fn test_message_close_frame_with_empty_reason() {
        let msg = Message::close_with(1000u16, "");
        let frame = msg.close_frame();
        assert_eq!(frame, Some((1000, "")));
    }

    // ==================== Clone 测试 ====================

    #[test]
    fn test_message_clone_text() {
        let msg1 = Message::text("hello");
        let msg2 = msg1.clone();
        assert_eq!(msg1.to_str().unwrap(), msg2.to_str().unwrap());
    }

    #[test]
    fn test_message_clone_binary() {
        let data = vec![1u8, 2, 3, 4];
        let msg1 = Message::binary(data.clone());
        let msg2 = msg1.clone();
        assert_eq!(msg1.as_bytes(), msg2.as_bytes());
    }

    // ==================== PartialEq 测试 ====================

    #[test]
    fn test_message_eq_text() {
        let msg1 = Message::text("hello");
        let msg2 = Message::text("hello");
        assert_eq!(msg1, msg2);
    }

    #[test]
    fn test_message_ne_text() {
        let msg1 = Message::text("hello");
        let msg2 = Message::text("world");
        assert_ne!(msg1, msg2);
    }

    #[test]
    fn test_message_eq_binary() {
        let data = vec![1u8, 2, 3, 4];
        let msg1 = Message::binary(data.clone());
        let msg2 = Message::binary(data);
        assert_eq!(msg1, msg2);
    }

    // ==================== 类型转换测试 ====================

    #[test]
    fn test_message_text_from_string() {
        let s = "hello".to_string();
        let msg = Message::text(s.clone());
        assert_eq!(msg.to_str().unwrap(), s);
    }

    #[test]
    fn test_message_text_from_str() {
        let msg = Message::text("hello");
        assert_eq!(msg.to_str().unwrap(), "hello");
    }

    #[test]
    fn test_message_binary_from_vec() {
        let data = vec![1u8, 2, 3, 4];
        let msg = Message::binary(data.clone());
        assert_eq!(msg.as_bytes(), data.as_slice());
    }

    #[test]
    fn test_message_binary_from_slice() {
        let data: &[u8] = &[1, 2, 3, 4];
        let msg = Message::binary(data);
        assert_eq!(msg.as_bytes(), data);
    }

    // ==================== Deref 测试 ====================

    #[test]
    fn test_message_deref() {
        let msg = Message::text("hello");
        // 通过 Deref 可以访问内部 protocol::Message 的方法
        assert!(msg.is_text());
    }

    // ==================== Debug 测试 ====================

    #[test]
    fn test_message_debug_text() {
        let msg = Message::text("hello");
        let debug_str = format!("{:?}", msg);
        // 验证 Debug 输出包含内容
        assert!(debug_str.contains("Text"));
    }

    #[test]
    fn test_message_debug_binary() {
        let msg = Message::binary(vec![1, 2, 3]);
        let debug_str = format!("{:?}", msg);
        // 验证 Debug 输出包含内容
        assert!(debug_str.contains("Binary"));
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_message_empty_text() {
        let msg = Message::text("");
        assert!(msg.is_text());
        assert_eq!(msg.to_str().unwrap(), "");
        assert_eq!(msg.as_bytes(), b"" as &[u8]);
    }

    #[test]
    fn test_message_empty_binary() {
        let msg = Message::binary(vec![]);
        assert!(msg.is_binary());
        assert_eq!(msg.as_bytes(), b"" as &[u8]);
    }

    #[test]
    fn test_message_empty_ping() {
        let msg = Message::ping(vec![]);
        assert!(msg.is_ping());
        assert_eq!(msg.as_bytes(), b"" as &[u8]);
    }

    #[test]
    fn test_message_empty_pong() {
        let msg = Message::pong(vec![]);
        assert!(msg.is_pong());
        assert_eq!(msg.as_bytes(), b"" as &[u8]);
    }

    #[test]
    fn test_message_unicode_text() {
        let msg = Message::text("你好，世界");
        assert_eq!(msg.to_str().unwrap(), "你好，世界");
    }

    #[test]
    fn test_message_large_binary() {
        // 创建 10000 字节的向量（用 0 填充）
        let data: Vec<u8> = vec![0u8; 10000];
        let msg = Message::binary(data.clone());
        assert_eq!(msg.as_bytes().len(), 10000);
        assert_eq!(msg.into_bytes(), data);
    }
}
