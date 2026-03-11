# taskq-rs

A distributed async task queue built in Rust, designed for reliability and clean architecture.

## What is this?

taskq-rs is a job processing system where producers enqueue work and workers consume it asynchronously. It targets **at-least-once delivery** with pluggable backends, configurable retry policies, and first-class observability.

Think Celery or Sidekiq, but in Rust with async/await and Tokio.

## Architecture

The system is organized as a Cargo workspace with clear separation of concerns:

```
taskq-rs/
├── taskq-core/              # Domain types, traits, error types
├── taskq-runtime/           # Worker pool, dispatcher, shutdown (WIP)
├── taskq-backend-memory/    # In-memory backend for dev/test (WIP)
├── taskq-backend-redis/     # Redis backend (planned)
└── taskq-backend-nats/      # NATS backend (planned)
```

### Core Traits

Everything is built around a small set of traits defined in `taskq-core`:

**`QueueBackend`** — pluggable storage layer

```rust
#[async_trait]
pub trait QueueBackend: Send + Sync {
    async fn enqueue(&self, task: Task) -> Result<TaskId, QueueError>;
    async fn reserve(&self, queue: &str) -> Result<Option<Task>, QueueError>;
    async fn ack(&self, id: &TaskId) -> Result<(), QueueError>;
    async fn nack(&self, id: &TaskId) -> Result<(), QueueError>;
    async fn move_to_dlq(&self, id: &TaskId) -> Result<(), QueueError>;
}
```

**`TaskHandler`** — user-defined processing logic

```rust
#[async_trait]
pub trait TaskHandler: Send + Sync {
    async fn handle(&self, task: &Task) -> Result<(), HandlerError>;
}
```

**`RetryPolicy`** — pure-logic retry decisions

```rust
pub trait RetryPolicy: Send + Sync {
    fn evaluate(&self, task: &Task) -> RetryDecision;
}
```

### Task Lifecycle

```
Pending ──reserve──▶ Active ──ack──▶ Completed
                       │
                      nack
                       │
                       ▼
                  Failed ──retry policy──▶ Pending (retry)
                       │
                       └──max attempts──▶ DeadLettered
```

### Task Model

Each task carries:

| Field | Type | Purpose |
|---|---|---|
| `id` | `TaskId` (UUID) | Unique identifier |
| `queue` | `String` | Routing key |
| `payload` | `Vec<u8>` | Opaque bytes — handlers deserialize as needed |
| `metadata` | `HashMap<String, String>` | Headers, trace context, custom tags |
| `status` | `TaskStatus` | Lifecycle state |
| `attempts` | `u32` | How many times this task has been tried |
| `max_attempts` | `u32` | Retry limit before dead-lettering |
| `created_at` | `DateTime<Utc>` | Creation timestamp |
| `scheduled_at` | `Option<DateTime<Utc>>` | Delayed execution (future use) |
| `visibility_deadline` | `Option<DateTime<Utc>>` | Lease expiry for reservation semantics |

## Design Goals

- **At-least-once delivery** — tasks are never silently lost
- **Pluggable backends** — swap between in-memory, Redis, or NATS without changing application code
- **Bounded concurrency** — backpressure is a first-class concern, not an afterthought
- **Graceful shutdown** — in-flight tasks complete before the process exits
- **Observability** — structured logs, tracing spans, and metrics built in
- **Testability** — the in-memory backend makes integration tests fast and deterministic

## Reliability Semantics

- Failed tasks are retried with exponential backoff (configurable via `RetryPolicy`)
- Tasks exceeding `max_attempts` are moved to a dead-letter queue
- Visibility deadlines prevent stuck tasks from blocking the queue forever
- Handlers should be idempotent — at-least-once delivery means duplicates are possible

## Building

```bash
cargo build
```

```bash
cargo clippy
```

```bash
cargo test
```

## Roadmap

- [x] **Phase 0** — Workspace scaffold, core types, trait definitions
- [ ] **Phase 1** — In-memory backend, worker pool, retries, dead-letter queue, basic tracing
- [ ] **Phase 2** — Redis backend, visibility timeouts, delayed retries, graceful shutdown, metrics
- [ ] **Phase 3** — NATS backend, OpenTelemetry export, leader election, circuit breaker

## License

MIT
