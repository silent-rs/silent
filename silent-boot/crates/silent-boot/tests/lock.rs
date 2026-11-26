//! M0 验证 V11：两个实例同时触发同一个任务时，执行锁保证只有一个实例执行。
//!
//! 用的是和迁移执行器同一把具名锁（`db::lock`），只是锁名换成 `job:<id>`。
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use silent_boot::db::lock::{LockError, LockOptions, acquire};

/// 任务实际执行的次数。
static RUNS: AtomicUsize = AtomicUsize::new(0);

/// 模拟一个实例到点执行任务：拿到锁才执行，执行期间一直持有。
async fn run_job_once(
    db: &DatabaseConnection,
    instance: &str,
    hold: Duration,
) -> Result<bool, LockError> {
    let options = LockOptions {
        timeout: Duration::from_millis(300),
        poll_interval: Duration::from_millis(20),
        ..LockOptions::default()
    };
    let guard = acquire(db, "job:demo", instance, &options).await;
    let Ok(guard) = guard else {
        return Ok(false);
    };
    RUNS.fetch_add(1, Ordering::SeqCst);
    tokio::time::sleep(hold).await;
    drop(guard);
    Ok(true)
}

/// 释放是 Drop 时交给运行时在后台做的，所以这里等一等再看锁表。
async fn wait_until_unlocked(db: &DatabaseConnection, name: &str) {
    for _ in 0..100 {
        let stmt = Statement::from_string(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) AS c FROM sb_locks WHERE name = '{name}'"),
        );
        let row = db.query_one_raw(stmt).await.expect("query").expect("row");
        if row.try_get::<i64>("", "c").expect("c") == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("lock {name} was not released");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v11_job_lock_lets_only_one_instance_run_the_job() {
    let (dir, db_a) = common::temp_db().await;
    let db_b = common::connect_again(&dir).await;

    // 两个实例同时到点。任务耗时 600 毫秒，等锁最多 300 毫秒：没抢到的那个直接放弃。
    let (ran_a, ran_b) = tokio::join!(
        run_job_once(&db_a, "instance-a", Duration::from_millis(600)),
        run_job_once(&db_b, "instance-b", Duration::from_millis(600)),
    );
    let (ran_a, ran_b) = (ran_a.expect("a"), ran_b.expect("b"));
    assert_ne!(ran_a, ran_b, "exactly one instance should run the job");
    assert_eq!(RUNS.load(Ordering::SeqCst), 1);

    // 任务跑完以后锁被释放，下一次到点谁都能执行。
    wait_until_unlocked(&db_a, "job:demo").await;
    assert!(
        run_job_once(&db_b, "instance-b", Duration::from_millis(1))
            .await
            .expect("second round")
    );
}

#[tokio::test]
async fn v11_names_do_not_interfere_and_expired_lease_can_be_taken_over() {
    let (_dir, db) = common::temp_db().await;
    let fast = LockOptions {
        timeout: Duration::from_millis(200),
        poll_interval: Duration::from_millis(20),
        ..LockOptions::default()
    };

    let held = acquire(&db, "job:demo", "a", &LockOptions::default())
        .await
        .expect("a takes demo");

    // 不同名字的锁互不影响。
    let other = acquire(&db, "job:other", "b", &fast).await.expect("other");
    drop(other);

    // 没到期时别人拿不到，报错里带着持有者。
    let err = acquire(&db, "job:demo", "b", &fast)
        .await
        .expect_err("demo is held");
    assert!(
        matches!(&err, LockError::Timeout { owner, .. } if owner == "a"),
        "{err}"
    );

    // 租约过期后可以接管。
    let stmt = Statement::from_string(
        DbBackend::Sqlite,
        "UPDATE sb_locks SET lease_until = 0 WHERE name = 'job:demo'".to_owned(),
    );
    db.execute_raw(stmt).await.expect("expire the lease");
    let taken = acquire(&db, "job:demo", "b", &fast)
        .await
        .expect("takeover");
    drop(taken);
    wait_until_unlocked(&db, "job:demo").await;
    drop(held);

    // 同一个持有者可以重入。
    let held = acquire(&db, "job:demo", "a", &fast).await.expect("a");
    let again = acquire(&db, "job:demo", "a", &fast).await.expect("a again");
    drop(again);
    drop(held);
}
