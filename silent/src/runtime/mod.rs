use std::future::Future;
use std::time::Duration;

pub use async_lock::RwLock;

pub mod mpsc {
    pub use futures::channel::mpsc::{
        Receiver, Sender, UnboundedReceiver, UnboundedSender, channel,
        unbounded as unbounded_channel,
    };
}

/// 运行一个异步任务（与具体运行时无关），不返回 JoinHandle。
/// 对于不需要持有句柄的场景，直接 detach。
pub fn spawn<F>(fut: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    async_global_executor::spawn(fut).detach();
}

/// 超时包装：在 duration 内未完成则返回 Err(())。
pub async fn timeout<T, F>(duration: Duration, fut: F) -> Result<T, ()>
where
    F: Future<Output = T>,
{
    use futures::future;
    let timer = async_io::Timer::after(duration);
    futures::pin_mut!(fut);
    futures::pin_mut!(timer);
    match future::select(fut, timer).await {
        future::Either::Left((val, _)) => Ok(val),
        future::Either::Right((_, _)) => Err(()),
    }
}
