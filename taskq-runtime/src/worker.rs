use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{instrument, Instrument};

use taskq_core::retry::{RetryDecision, RetryPolicy};
use taskq_core::task::Task;
use taskq_core::{QueueBackend, TaskHandler};

use crate::config::WorkerPoolConfig;

/// Run a single worker loop: poll for tasks, handle them, ack/nack.
///
/// The worker exits when the cancellation token is cancelled and no task
/// is currently being processed.
#[instrument(skip_all, fields(worker_id = worker_id, queue = %config.queue))]
pub(crate) async fn run_worker<B, H, R>(
    worker_id: usize,
    config: Arc<WorkerPoolConfig>,
    backend: Arc<B>,
    handler: Arc<H>,
    retry_policy: Arc<R>,
    cancel: CancellationToken,
) where
    B: QueueBackend,
    H: TaskHandler,
    R: RetryPolicy,
{
    tracing::info!("worker started");

    loop {
        if cancel.is_cancelled() {
            tracing::info!("worker shutting down");
            return;
        }

        let task = match backend.reserve(&config.queue).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                // No work available — wait with cancellation awareness.
                tokio::select! {
                    () = tokio::time::sleep(config.poll_interval) => {}
                    () = cancel.cancelled() => {}
                }
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, "reserve failed");
                tokio::select! {
                    () = tokio::time::sleep(config.poll_interval) => {}
                    () = cancel.cancelled() => {}
                }
                continue;
            }
        };

        process_task(&task, backend.as_ref(), handler.as_ref(), retry_policy.as_ref())
            .instrument(tracing::info_span!(
                "process_task",
                task_id = %task.id,
                attempt = task.attempts + 1,
            ))
            .await;
    }
}

/// Handle a single task: invoke the handler, then ack or nack/DLQ.
async fn process_task<B, H, R>(task: &Task, backend: &B, handler: &H, retry_policy: &R)
where
    B: QueueBackend,
    H: TaskHandler,
    R: RetryPolicy,
{
    match handler.handle(task).await {
        Ok(()) => {
            tracing::info!("task succeeded");
            if let Err(e) = backend.ack(&task.id).await {
                tracing::error!(error = %e, "ack failed");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "task failed");

            // Build a view of the task with attempts incremented for policy evaluation.
            // The backend's nack will also increment attempts on the stored copy.
            let mut eval_task = task.clone();
            eval_task.attempts += 1;

            match retry_policy.evaluate(&eval_task) {
                RetryDecision::Retry { delay } => {
                    let retry_after = Utc::now() + chrono::Duration::from_std(delay).unwrap();
                    tracing::info!(?delay, "scheduling retry");
                    if let Err(e) = backend.nack(&task.id, Some(retry_after)).await {
                        tracing::error!(error = %e, "nack failed");
                    }
                }
                RetryDecision::MoveToDeadLetterQueue => {
                    tracing::warn!("moving task to DLQ");
                    if let Err(e) = backend.move_to_dlq(&task.id).await {
                        tracing::error!(error = %e, "move_to_dlq failed");
                    }
                }
            }
        }
    }
}
