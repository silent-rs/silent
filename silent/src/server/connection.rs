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
