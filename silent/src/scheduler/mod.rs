pub(crate) mod middleware;
mod process_time;
mod task;
pub mod traits;

use anyhow::{Result, anyhow};
use async_lock::Mutex;
use std::sync::{Arc, LazyLock};
use std::thread;
use tracing::{error, info};

pub use process_time::ProcessTime;
pub use task::Task;
pub use traits::SchedulerExt;

pub static SCHEDULER: LazyLock<Arc<Mutex<Scheduler>>> =
    LazyLock::new(|| Arc::new(Mutex::new(Scheduler::new())));

#[derive(Debug, Clone)]
pub struct Scheduler {
    tasks: Vec<Task>,
    schedule: bool,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            schedule: true,
        }
    }

    pub fn add_task(&mut self, task: Task) -> Result<()> {
        if self.tasks.iter().any(|t| t.id == task.id) {
            return Err(anyhow!(format!("task {id} already exists!", id = task.id)));
        }
        info!(
            "task: ID:{:?} Description:{:?} ProcessTime:{:?} add success!",
            task.id, task.description, task.process_time
        );
        self.tasks.push(task);
        Ok(())
    }

    pub fn remove_task(&mut self, id: &str) {
        info!("task: ID:{:?} remove success!", id);
        self.tasks.retain(|t| t.id != id);
    }

    pub fn remove_task_sub(&mut self, id: &str) {
        info!("sub task: ID:{:?} remove success!", id);
        self.tasks.retain(|t| t.id.starts_with(id));
    }

    pub fn get_task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_tasks(&self) -> &Vec<Task> {
        &self.tasks
    }
    pub async fn run(&mut self) {
        let mut removable_list = Vec::new();
        for task in self.tasks.clone() {
            if task.is_removable() {
                removable_list.push(task.id.clone());
            }
            if task.is_async {
                async_global_executor::spawn(async move {
                    match task.clone().run_async().await {
                        Ok(_) => {}
                        Err(e) => error!(
                            "task: ID:{:?} Description:{:?} ProcessTime:{:?} run failed! error: {:?}",
                            task.id, task.description, task.process_time, e
                        ),
                    }
                }).detach();
            } else {
                thread::spawn(move || match task.clone().run() {
                    Ok(_) => {}
                    Err(e) => error!(
                        "task: ID:{:?} Description:{:?} ProcessTime:{:?} run failed! error: {:?}",
                        task.id, task.description, task.process_time, e
                    ),
                });
            }
        }
        for id in removable_list {
            self.remove_task(&id);
        }
    }

    pub fn stop(&mut self) {
        self.schedule = false;
    }

    pub async fn schedule(schedule: Arc<Mutex<Self>>) {
        loop {
            schedule.lock().await.run().await;
            async_io::Timer::after(std::time::Duration::from_secs(1)).await;
            if !schedule.lock().await.schedule {
                schedule.lock().await.schedule = true;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lock::Mutex;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::scheduler::Scheduler;
    use crate::scheduler::process_time::ProcessTime;
    use crate::scheduler::task::Task;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_scheduler_async() {
        let mut scheduler = Scheduler::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone1 = counter.clone();
        let sync_task = Task::create_with_action(
            "sync_task".to_string(),
            ProcessTime::try_from("2015-01-01T00:00:00Z".to_string()).unwrap(),
            "sync_task".to_string(),
            Arc::new(move || {
                counter_clone1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        );
        scheduler.add_task(sync_task).unwrap();
        let counter_clone2 = counter.clone();
        let async_task = Task::create_with_action_async(
            "async_task".to_string(),
            ProcessTime::try_from("2015-01-01T00:00:00Z".to_string()).unwrap(),
            "async_task".to_string(),
            Arc::new(move || {
                let counter_clone = counter_clone2.clone();
                Box::pin(async move {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        );
        scheduler.add_task(async_task.clone()).unwrap();
        assert_eq!(scheduler.get_tasks().len(), 2);
        assert!(scheduler.add_task(async_task.clone()).is_err());
        scheduler.remove_task(&async_task.id);
        scheduler.add_task(async_task).unwrap();
        assert!(scheduler.get_task("async_task").is_some());
        let arc_scheduler = Arc::new(Mutex::new(scheduler));
        let arc_scheduler_clone = arc_scheduler.clone();
        tokio::spawn(async move {
            Scheduler::schedule(arc_scheduler_clone).await;
        });
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        arc_scheduler.lock().await.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_scheduler() {
        let mut scheduler = Scheduler::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        // 使用每秒执行的cron表达式，确保在测试期间会执行多次
        let async_task = Task::create_with_action_async(
            "async_task".to_string(),
            ProcessTime::try_from("* * * * * * *".to_string()).unwrap(),
            "async_task".to_string(),
            Arc::new(move || {
                let counter_clone = counter_clone.clone();
                Box::pin(async move {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        );
        scheduler.add_task(async_task).unwrap();
        assert!(scheduler.get_task("async_task").is_some());
        let arc_scheduler = Arc::new(Mutex::new(scheduler));
        let arc_scheduler_clone = arc_scheduler.clone();
        tokio::spawn(async move {
            Scheduler::schedule(arc_scheduler_clone).await;
        });
        // 等待3秒，任务应该执行约3次
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        // 检查至少执行了2次（考虑到调度延迟）
        let count = counter.load(Ordering::SeqCst);
        assert!(count >= 2, "Expected at least 2 executions, got {}", count);
        arc_scheduler.lock().await.stop();
    }
}
