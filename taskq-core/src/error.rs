use crate::task::TaskId;

/// Errors returned by queue backend operations.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// The requested task does not exist in the backend.
    #[error("task not found: {0}")]
    TaskNotFound(TaskId),

    /// The queue has reached its capacity limit.
    #[error("queue is full")]
    QueueFull,

    /// An error originating from the underlying backend implementation.
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Errors returned by task handlers during execution.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HandlerError(#[source] pub Box<dyn std::error::Error + Send + Sync>);

impl HandlerError {
    /// Wrap any error into a `HandlerError`.
    pub fn new(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(err))
    }
}
