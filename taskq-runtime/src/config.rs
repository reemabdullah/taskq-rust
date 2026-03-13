use std::time::Duration;

/// Configuration for the worker pool.
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    /// The queue name to poll for tasks.
    pub queue: String,
    /// Number of concurrent worker tasks.
    pub concurrency: usize,
    /// How often workers poll the backend when idle.
    pub poll_interval: Duration,
    /// Maximum time to wait for in-flight tasks during shutdown.
    pub shutdown_timeout: Duration,
}

impl WorkerPoolConfig {
    /// Create a new config for the given queue with sensible defaults.
    pub fn new(queue: impl Into<String>) -> Self {
        Self {
            queue: queue.into(),
            concurrency: 4,
            poll_interval: Duration::from_millis(100),
            shutdown_timeout: Duration::from_secs(30),
        }
    }

    /// Set the number of concurrent workers.
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    /// Set the poll interval.
    pub fn poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    /// Set the graceful shutdown timeout.
    pub fn shutdown_timeout(mut self, d: Duration) -> Self {
        self.shutdown_timeout = d;
        self
    }
}
