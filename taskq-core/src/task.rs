use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type-safe wrapper around a UUID identifying a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(Uuid);

impl TaskId {
    /// Generate a new random task ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Lifecycle status of a task in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Waiting to be picked up by a worker.
    Pending,
    /// Currently leased to a worker for processing.
    Active,
    /// Successfully completed.
    Completed,
    /// Failed (may be retried depending on policy).
    Failed,
    /// Permanently failed and moved to the dead-letter queue.
    DeadLettered,
}

/// A unit of work to be processed by the task queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for this task.
    pub id: TaskId,
    /// The queue this task belongs to.
    pub queue: String,
    /// Opaque payload bytes — handlers deserialize as needed.
    pub payload: Vec<u8>,
    /// Arbitrary key-value metadata (headers, trace context, etc.).
    pub metadata: HashMap<String, String>,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// Number of times this task has been attempted.
    pub attempts: u32,
    /// Maximum number of attempts before moving to the DLQ.
    pub max_attempts: u32,
    /// When the task was created.
    pub created_at: DateTime<Utc>,
    /// Optional delayed execution time.
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Lease expiry — if a worker doesn't ack before this, the task can be reclaimed.
    pub visibility_deadline: Option<DateTime<Utc>>,
}

impl Task {
    /// Create a new task with sensible defaults.
    ///
    /// Status is `Pending`, attempts is 0, and timestamps are set to now.
    pub fn new(queue: impl Into<String>, payload: Vec<u8>, max_attempts: u32) -> Self {
        Self {
            id: TaskId::new(),
            queue: queue.into(),
            payload,
            metadata: HashMap::new(),
            status: TaskStatus::Pending,
            attempts: 0,
            max_attempts,
            created_at: Utc::now(),
            scheduled_at: None,
            visibility_deadline: None,
        }
    }
}
