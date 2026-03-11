use std::time::Duration;

use crate::task::Task;

/// The outcome of evaluating a retry policy for a failed task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry the task after the specified delay.
    Retry { delay: Duration },
    /// The task has exhausted its retries and should be moved to the DLQ.
    MoveToDeadLetterQueue,
}

/// Trait for determining whether a failed task should be retried or dead-lettered.
///
/// Implementations are synchronous — retry decisions are pure logic
/// that should not require I/O.
pub trait RetryPolicy: Send + Sync {
    /// Evaluate whether the given task should be retried or moved to the DLQ.
    fn evaluate(&self, task: &Task) -> RetryDecision;
}
