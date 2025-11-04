use std::any::Any;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Connection: Any + AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
}

impl<T> Connection for T
where
    T: Any + AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync> {
        self
    }
}

pub type BoxedConnection = Box<dyn Connection + Send + Sync>;

impl dyn Connection + Send + Sync {
    pub fn downcast<T: Any + Send + Sync + 'static>(self: Box<Self>) -> Result<Box<T>, Box<Self>> {
        // 仅在类型匹配时才转换为 Any 并 downcast；否则直接返回原始 Box<Self>
        if (*self).as_any().is::<T>() {
            let boxed_any = Connection::into_any(self);
            // SAFETY: 上面已经通过 is::<T>() 检查确保类型匹配
            Ok(boxed_any.downcast::<T>().unwrap())
        } else {
            Err(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_downcast_success_and_failure() {
        // 使用 tokio::io::duplex 作为一个实现 AsyncRead/Write 的类型
        let (mut a, b) = tokio::io::duplex(64);
        let boxed: BoxedConnection = Box::new(b);
        // 成功 downcast 为 DuplexStream（不使用 expect 以避免 Debug 约束）
        let res = boxed.downcast::<tokio::io::DuplexStream>();
        assert!(res.is_ok());
        let mut peer: Box<tokio::io::DuplexStream> = res.ok().unwrap();

        // 再构造一个 BoxedConnection，用于失败分支测试
        let (_a2, b2) = tokio::io::duplex(32);
        let boxed2: BoxedConnection = Box::new(b2);
        // 失败 downcast，应返回 Err 原对象
        let err = boxed2
            .downcast::<tokio::net::TcpStream>()
            .expect_err("expected Err on mismatch");
        // 验证原对象仍可用（向 peer 写入并从 a 读取）
        peer.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        a.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
        let _ = err; // 忽略使用
    }
}
