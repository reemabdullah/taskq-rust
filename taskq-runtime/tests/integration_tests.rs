use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use taskq_backend_memory::InMemoryBackend;
use taskq_core::error::HandlerError;
use taskq_core::task::Task;
use taskq_core::{ExponentialBackoff, QueueBackend, TaskHandler};
use taskq_runtime::{WorkerPool, WorkerPoolConfig};

// ---------------------------------------------------------------------------
// Test handlers
// ---------------------------------------------------------------------------

/// Always succeeds, counting invocations.
struct SuccessHandler {
    count: Arc<AtomicU32>,
}

impl SuccessHandler {
    fn new() -> (Self, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (Self { count: count.clone() }, count)
    }
}

#[async_trait]
impl TaskHandler for SuccessHandler {
    async fn handle(&self, _task: &Task) -> Result<(), HandlerError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Always fails with a fixed message.
struct FailHandler;

#[async_trait]
impl TaskHandler for FailHandler {
    async fn handle(&self, _task: &Task) -> Result<(), HandlerError> {
        Err(HandlerError::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "always fails",
        )))
    }
}

/// Fails the first N invocations, then succeeds.
struct FailNThenSucceed {
    fail_count: u32,
    invocations: AtomicU32,
    success_count: Arc<AtomicU32>,
}

impl FailNThenSucceed {
    fn new(fail_count: u32) -> (Self, Arc<AtomicU32>) {
        let success_count = Arc::new(AtomicU32::new(0));
        (
            Self {
                fail_count,
                invocations: AtomicU32::new(0),
                success_count: success_count.clone(),
            },
            success_count,
        )
    }
}

#[async_trait]
impl TaskHandler for FailNThenSucceed {
    async fn handle(&self, _task: &Task) -> Result<(), HandlerError> {
        let n = self.invocations.fetch_add(1, Ordering::SeqCst);
        if n < self.fail_count {
            Err(HandlerError::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("fail #{}", n + 1),
            )))
        } else {
            self.success_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn retry_policy() -> ExponentialBackoff {
    // Very short delays for fast tests.
    ExponentialBackoff::new(Duration::from_millis(10), Duration::from_millis(50))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn task_success_path() {
    let backend = InMemoryBackend::new();
    let task = Task::new("work", b"payload".to_vec(), 3);
    let id = backend.enqueue(task).await.unwrap();

    let (handler, count) = SuccessHandler::new();
    let config = WorkerPoolConfig::new("work")
        .concurrency(1)
        .poll_interval(Duration::from_millis(10))
        .shutdown_timeout(Duration::from_secs(5));

    let pool = WorkerPool::start(config, backend.clone(), handler, retry_policy());

    // Wait for the task to be processed.
    tokio::time::sleep(Duration::from_millis(200)).await;

    pool.shutdown().await.unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 1);
    let stored = backend.get_task(&id).unwrap();
    assert_eq!(stored.status, taskq_core::TaskStatus::Completed);
}

#[tokio::test]
async fn failure_then_retry_then_success() {
    let backend = InMemoryBackend::new();
    let task = Task::new("work", vec![], 5);
    let id = backend.enqueue(task).await.unwrap();

    let (handler, success_count) = FailNThenSucceed::new(2); // fail twice, then succeed
    let config = WorkerPoolConfig::new("work")
        .concurrency(1)
        .poll_interval(Duration::from_millis(10))
        .shutdown_timeout(Duration::from_secs(5));

    let pool = WorkerPool::start(config, backend.clone(), handler, retry_policy());

    // Wait long enough for retries + delays.
    tokio::time::sleep(Duration::from_millis(500)).await;

    pool.shutdown().await.unwrap();

    assert_eq!(success_count.load(Ordering::SeqCst), 1);
    let stored = backend.get_task(&id).unwrap();
    assert_eq!(stored.status, taskq_core::TaskStatus::Completed);
}

#[tokio::test]
async fn exhausted_retries_moves_to_dlq() {
    let backend = InMemoryBackend::new();
    let task = Task::new("work", vec![], 2); // max 2 attempts
    backend.enqueue(task).await.unwrap();

    let config = WorkerPoolConfig::new("work")
        .concurrency(1)
        .poll_interval(Duration::from_millis(10))
        .shutdown_timeout(Duration::from_secs(5));

    let pool = WorkerPool::start(config, backend.clone(), FailHandler, retry_policy());

    tokio::time::sleep(Duration::from_millis(500)).await;

    pool.shutdown().await.unwrap();

    assert_eq!(backend.dlq_depth("work"), 1);
    assert_eq!(backend.queue_depth("work"), 0);
}

#[tokio::test]
async fn graceful_shutdown_completes() {
    let backend = InMemoryBackend::new();
    let (handler, _count) = SuccessHandler::new();

    let config = WorkerPoolConfig::new("work")
        .concurrency(2)
        .poll_interval(Duration::from_millis(10))
        .shutdown_timeout(Duration::from_secs(5));

    let pool = WorkerPool::start(config, backend, handler, retry_policy());

    // Shut down immediately — workers should exit cleanly even with no tasks.
    let result = pool.shutdown().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn backoff_delays_retry() {
    let backend = InMemoryBackend::new();
    let task = Task::new("work", vec![], 5);
    let id = backend.enqueue(task).await.unwrap();

    // Use a longer base delay so we can observe that the task is not
    // immediately available after a nack.
    let policy = ExponentialBackoff::new(Duration::from_millis(300), Duration::from_secs(5));

    let (handler, success_count) = FailNThenSucceed::new(1);
    let config = WorkerPoolConfig::new("work")
        .concurrency(1)
        .poll_interval(Duration::from_millis(10))
        .shutdown_timeout(Duration::from_secs(5));

    let pool = WorkerPool::start(config, backend.clone(), handler, policy);

    // After 50ms, the task should have failed once and be scheduled ~300ms in the future.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(success_count.load(Ordering::SeqCst), 0);

    // After 500ms total, it should have been retried and succeeded.
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_eq!(success_count.load(Ordering::SeqCst), 1);

    pool.shutdown().await.unwrap();

    let stored = backend.get_task(&id).unwrap();
    assert_eq!(stored.status, taskq_core::TaskStatus::Completed);
}
