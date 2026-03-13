use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::instrument;

use taskq_core::error::QueueError;
use taskq_core::task::{Task, TaskId, TaskStatus};
use taskq_core::QueueBackend;

/// Configuration for the in-memory backend.
#[derive(Debug, Clone, Default)]
pub struct InMemoryBackendConfig {
    /// Maximum number of tasks per queue. `None` means unbounded.
    pub max_queue_size: Option<usize>,
}

/// Internal mutable state guarded by a `Mutex`.
#[derive(Debug)]
struct State {
    /// All tasks by ID.
    tasks: HashMap<TaskId, Task>,
    /// Per-queue FIFO of task IDs.
    queues: HashMap<String, VecDeque<TaskId>>,
    /// Per-queue dead-letter storage.
    dlq: HashMap<String, Vec<TaskId>>,
    /// Capacity limit per queue.
    max_queue_size: Option<usize>,
}

/// An in-memory `QueueBackend` for local development and testing.
///
/// Uses `std::sync::Mutex` (not tokio) because no `.await` is held
/// under the lock — all operations are O(n) in queue depth at worst.
#[derive(Debug, Clone)]
pub struct InMemoryBackend {
    state: Arc<Mutex<State>>,
}

impl InMemoryBackend {
    /// Create a new in-memory backend with default (unbounded) configuration.
    pub fn new() -> Self {
        Self::with_config(InMemoryBackendConfig::default())
    }

    /// Create a new in-memory backend with the given configuration.
    pub fn with_config(config: InMemoryBackendConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                tasks: HashMap::new(),
                queues: HashMap::new(),
                dlq: HashMap::new(),
                max_queue_size: config.max_queue_size,
            })),
        }
    }

    /// Return the number of pending/active tasks in the given queue.
    pub fn queue_depth(&self, queue: &str) -> usize {
        let state = self.state.lock().unwrap();
        state.queues.get(queue).map_or(0, |q| q.len())
    }

    /// Return the number of dead-lettered tasks for the given queue.
    pub fn dlq_depth(&self, queue: &str) -> usize {
        let state = self.state.lock().unwrap();
        state.dlq.get(queue).map_or(0, |d| d.len())
    }

    /// Retrieve a clone of a task by ID (for test assertions).
    pub fn get_task(&self, id: &TaskId) -> Option<Task> {
        let state = self.state.lock().unwrap();
        state.tasks.get(id).cloned()
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QueueBackend for InMemoryBackend {
    #[instrument(skip(self, task), fields(task_id = %task.id, queue = %task.queue))]
    async fn enqueue(&self, task: Task) -> Result<TaskId, QueueError> {
        let mut state = self.state.lock().unwrap();
        let queue_name = task.queue.clone();
        let id = task.id;

        // Check capacity.
        if let Some(max) = state.max_queue_size {
            let depth = state.queues.get(&queue_name).map_or(0, |q| q.len());
            if depth >= max {
                return Err(QueueError::QueueFull);
            }
        }

        state.tasks.insert(id, task);
        state
            .queues
            .entry(queue_name)
            .or_default()
            .push_back(id);

        tracing::debug!("task enqueued");
        Ok(id)
    }

    #[instrument(skip(self), fields(queue = %queue))]
    async fn reserve(&self, queue: &str) -> Result<Option<Task>, QueueError> {
        let mut state = self.state.lock().unwrap();
        let now = Utc::now();

        // Destructure to get disjoint borrows on the struct fields.
        let State {
            ref mut queues,
            ref tasks,
            ..
        } = *state;

        let deque = match queues.get_mut(queue) {
            Some(d) => d,
            None => return Ok(None),
        };

        // Scan for the first eligible task (Pending + scheduled_at <= now or None).
        let pos = deque.iter().position(|id| {
            if let Some(task) = tasks.get(id) {
                if task.status != TaskStatus::Pending {
                    return false;
                }
                match task.scheduled_at {
                    Some(at) => at <= now,
                    None => true,
                }
            } else {
                false
            }
        });

        let pos = match pos {
            Some(p) => p,
            None => return Ok(None),
        };

        let id = deque.remove(pos).unwrap();
        let task = state.tasks.get_mut(&id).unwrap();
        task.status = TaskStatus::Active;

        let clone = task.clone();
        tracing::debug!(task_id = %id, "task reserved");
        Ok(Some(clone))
    }

    #[instrument(skip(self), fields(task_id = %id))]
    async fn ack(&self, id: &TaskId) -> Result<(), QueueError> {
        let mut state = self.state.lock().unwrap();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or(QueueError::TaskNotFound(*id))?;

        task.status = TaskStatus::Completed;
        tracing::debug!("task acknowledged");
        Ok(())
    }

    #[instrument(skip(self), fields(task_id = %id))]
    async fn nack(
        &self,
        id: &TaskId,
        retry_after: Option<DateTime<Utc>>,
    ) -> Result<(), QueueError> {
        let mut state = self.state.lock().unwrap();

        let (queue_name, attempts) = {
            let task = state
                .tasks
                .get_mut(id)
                .ok_or(QueueError::TaskNotFound(*id))?;

            task.attempts += 1;
            task.status = TaskStatus::Pending;
            task.scheduled_at = retry_after;

            (task.queue.clone(), task.attempts)
        };

        state
            .queues
            .entry(queue_name)
            .or_default()
            .push_back(*id);

        tracing::debug!(attempts, "task nacked and re-enqueued");
        Ok(())
    }

    #[instrument(skip(self), fields(task_id = %id))]
    async fn move_to_dlq(&self, id: &TaskId) -> Result<(), QueueError> {
        let mut state = self.state.lock().unwrap();

        let queue_name = {
            let task = state
                .tasks
                .get_mut(id)
                .ok_or(QueueError::TaskNotFound(*id))?;

            task.status = TaskStatus::DeadLettered;
            task.queue.clone()
        };

        state.dlq.entry(queue_name).or_default().push(*id);

        tracing::debug!("task moved to DLQ");
        Ok(())
    }
}
