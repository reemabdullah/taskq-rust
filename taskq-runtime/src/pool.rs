use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use taskq_core::retry::RetryPolicy;
use taskq_core::{QueueBackend, TaskHandler};

use crate::config::WorkerPoolConfig;
use crate::worker::run_worker;

/// A pool of concurrent worker tasks that process jobs from a queue.
pub struct WorkerPool {
    handles: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
    shutdown_timeout: std::time::Duration,
}

impl WorkerPool {
    /// Spawn a pool of workers and begin processing tasks immediately.
    pub fn start<B, H, R>(
        config: WorkerPoolConfig,
        backend: B,
        handler: H,
        retry_policy: R,
    ) -> Self
    where
        B: QueueBackend + 'static,
        H: TaskHandler + 'static,
        R: RetryPolicy + 'static,
    {
        let cancel = CancellationToken::new();
        let config = Arc::new(config);
        let backend = Arc::new(backend);
        let handler = Arc::new(handler);
        let retry_policy = Arc::new(retry_policy);

        let mut handles = Vec::with_capacity(config.concurrency);

        for worker_id in 0..config.concurrency {
            let config = Arc::clone(&config);
            let backend = Arc::clone(&backend);
            let handler = Arc::clone(&handler);
            let retry_policy = Arc::clone(&retry_policy);
            let cancel = cancel.clone();

            let handle = tokio::spawn(async move {
                run_worker(worker_id, config, backend, handler, retry_policy, cancel).await;
            });
            handles.push(handle);
        }

        tracing::info!(
            concurrency = config.concurrency,
            queue = %config.queue,
            "worker pool started"
        );

        Self {
            handles,
            cancel,
            shutdown_timeout: config.shutdown_timeout,
        }
    }

    /// Get a clone of the cancellation token for external shutdown signaling.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Initiate graceful shutdown: cancel workers, then await completion
    /// with a timeout.
    ///
    /// Returns `Ok(())` if all workers finished within the timeout,
    /// or `Err(())` if the timeout was exceeded.
    pub async fn shutdown(self) -> Result<(), ()> {
        tracing::info!("initiating worker pool shutdown");
        self.cancel.cancel();

        let join_all = async {
            for (i, handle) in self.handles.into_iter().enumerate() {
                if let Err(e) = handle.await {
                    tracing::error!(worker_id = i, error = %e, "worker panicked");
                }
            }
        };

        match tokio::time::timeout(self.shutdown_timeout, join_all).await {
            Ok(()) => {
                tracing::info!("worker pool shut down gracefully");
                Ok(())
            }
            Err(_) => {
                tracing::warn!("shutdown timed out, some workers may still be running");
                Err(())
            }
        }
    }
}
