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

/// Exponential backoff retry policy.
///
/// Computes delay as `min(base_delay * 2^(attempts - 1), max_delay)`.
/// Once `task.attempts >= task.max_attempts`, the task is moved to the DLQ.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    /// The delay used for the first retry.
    pub base_delay: Duration,
    /// Upper bound on the computed delay.
    pub max_delay: Duration,
}

impl ExponentialBackoff {
    /// Create a new `ExponentialBackoff` policy.
    pub fn new(base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            base_delay,
            max_delay,
        }
    }
}

impl RetryPolicy for ExponentialBackoff {
    fn evaluate(&self, task: &Task) -> RetryDecision {
        if task.attempts >= task.max_attempts {
            return RetryDecision::MoveToDeadLetterQueue;
        }

        // attempts is the count *after* the current failure, so exponent is attempts - 1.
        // On the first failure attempts == 1, exponent == 0 → delay == base_delay.
        let exponent = task.attempts.saturating_sub(1);
        let multiplier = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let delay = self
            .base_delay
            .saturating_mul(multiplier as u32)
            .min(self.max_delay);

        RetryDecision::Retry { delay }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;

    fn task_with_attempts(attempts: u32, max_attempts: u32) -> Task {
        let mut task = Task::new("test-queue", vec![], max_attempts);
        task.attempts = attempts;
        task
    }

    #[test]
    fn first_retry_uses_base_delay() {
        let policy = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(60),
        );
        let task = task_with_attempts(1, 5);
        assert_eq!(
            policy.evaluate(&task),
            RetryDecision::Retry {
                delay: Duration::from_secs(1)
            }
        );
    }

    #[test]
    fn second_retry_doubles_delay() {
        let policy = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(60),
        );
        let task = task_with_attempts(2, 5);
        assert_eq!(
            policy.evaluate(&task),
            RetryDecision::Retry {
                delay: Duration::from_secs(2)
            }
        );
    }

    #[test]
    fn third_retry_quadruples_delay() {
        let policy = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(60),
        );
        let task = task_with_attempts(3, 5);
        assert_eq!(
            policy.evaluate(&task),
            RetryDecision::Retry {
                delay: Duration::from_secs(4)
            }
        );
    }

    #[test]
    fn delay_caps_at_max() {
        let policy = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        // 2^9 = 512 seconds, but max is 5
        let task = task_with_attempts(10, 20);
        assert_eq!(
            policy.evaluate(&task),
            RetryDecision::Retry {
                delay: Duration::from_secs(5)
            }
        );
    }

    #[test]
    fn exhausted_attempts_moves_to_dlq() {
        let policy = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(60),
        );
        let task = task_with_attempts(3, 3);
        assert_eq!(policy.evaluate(&task), RetryDecision::MoveToDeadLetterQueue);
    }

    #[test]
    fn zero_attempts_with_zero_max_moves_to_dlq() {
        let policy = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(60),
        );
        let task = task_with_attempts(0, 0);
        assert_eq!(policy.evaluate(&task), RetryDecision::MoveToDeadLetterQueue);
    }
}
