use async_trait::async_trait;

use crate::error::QueueError;
use crate::task::{Task, TaskId};

/// Trait representing a pluggable queue storage backend.
///
/// Implementations handle the persistence and retrieval of tasks.
/// The trait is object-safe (`dyn QueueBackend`) to allow runtime backend selection.
#[async_trait]
pub trait QueueBackend: Send + Sync {
    /// Insert a new task into the queue. Returns the assigned task ID.
    async fn enqueue(&self, task: Task) -> Result<TaskId, QueueError>;

    /// Reserve the next available task from the given queue for processing.
    ///
    /// Returns `Ok(None)` when the queue is empty.
    async fn reserve(&self, queue: &str) -> Result<Option<Task>, QueueError>;

    /// Acknowledge successful completion of a task.
    async fn ack(&self, id: &TaskId) -> Result<(), QueueError>;

    /// Negatively acknowledge a task, signaling processing failure.
    ///
    /// The backend should make the task available for retry.
    async fn nack(&self, id: &TaskId) -> Result<(), QueueError>;

    /// Move a task to the dead-letter queue after exhausting retries.
    async fn move_to_dlq(&self, id: &TaskId) -> Result<(), QueueError>;
}
