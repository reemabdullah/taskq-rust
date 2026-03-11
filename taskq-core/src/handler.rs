use async_trait::async_trait;

use crate::error::HandlerError;
use crate::task::Task;

/// Trait for user-defined task processing logic.
///
/// Implementations should be idempotent where possible, since
/// the queue provides at-least-once delivery semantics.
#[async_trait]
pub trait TaskHandler: Send + Sync {
    /// Process a task. Return `Ok(())` on success, or a `HandlerError` on failure.
    async fn handle(&self, task: &Task) -> Result<(), HandlerError>;
}
