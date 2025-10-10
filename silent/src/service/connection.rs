use std::any::TypeId;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Connection: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl<T> Connection for T where T: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static {}

pub type BoxedConnection = Box<dyn Connection + Send + Sync>;

impl dyn Connection + Send + Sync {
    pub fn is<T: Connection>(&self) -> bool {
        self.type_id() == TypeId::of::<T>()
    }

    pub fn downcast<T: Connection>(self: Box<Self>) -> Result<Box<T>, Box<Self>> {
        if self.is::<T>() {
            let raw = Box::into_raw(self) as *mut T;
            // SAFETY: type_id check ensures cast is valid
            Ok(unsafe { Box::from_raw(raw) })
        } else {
            Err(self)
        }
    }
}
