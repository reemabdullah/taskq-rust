# CLAUDE.md

## Project Overview

This project explores the design of a distributed task queue in Rust, focusing on reliability, backpressure, and observability in async systems.

It also serves as a practical environment for experimenting with backend systems design and clean Rust architecture.

The goal is to build a production-style queueing system that demonstrates:

- async Rust with Tokio
- worker pools
- retries with exponential backoff
- dead-letter queues
- backpressure handling
- graceful shutdown
- observability with tracing and OpenTelemetry
- pluggable broker backends (Redis and/or NATS)
- at-least-once delivery semantics

---

## High-Level Product Goals

The system should allow producers to enqueue jobs and workers to consume and execute them asynchronously.

Key requirements:

- enqueue task
- reserve/fetch task for processing
- acknowledge success
- retry failed tasks with exponential backoff
- move permanently failed tasks to a dead-letter queue
- support worker concurrency
- support graceful shutdown
- expose observability signals:
  - structured logs
  - traces
  - metrics
- enforce backpressure so the system degrades safely under load

Stretch goals:

- leader election for scheduled recovery/requeue tasks
- circuit breaker around backend operations
- admin/debug endpoints
- task scheduling / delayed execution
- idempotency keys
- dashboard or CLI inspection tool

---

## Architecture Principles

Follow these principles strictly:

### 1. Clean separation of concerns

Separate:

- domain logic
- broker abstraction
- worker runtime
- retry policy
- observability
- configuration

Do not mix transport/storage logic with business logic.

### 2. Traits first

Prefer traits and clear interfaces for pluggable components:

- QueueBackend
- RetryPolicy
- TaskHandler
- BackpressurePolicy
- LeaderElector

### 3. Small modules

Keep modules focused and small.
Avoid giant files.
If a file grows too large, propose a refactor.

### 4. Production-minded design

Prioritize:

- clear error types
- timeouts
- retries
- bounded concurrency
- cancellation safety
- graceful shutdown
- structured logging
- testability

### 5. Explicit tradeoffs

Whenever making an architectural decision, explain:

- why this approach was chosen
- alternatives considered
- tradeoffs
- future extensions

### 6. Avoid overengineering

Start with the simplest correct implementation.
Build incrementally.
Do not add advanced distributed features until the core queue works end to end.

---

## Expected Initial Scope

Phase 1:

- in-memory backend for fast local development
- task model
- worker pool
- ack / nack
- retries
- dead-letter queue
- basic tracing/logging

Phase 2:

- Redis backend
- visibility timeout / reservation semantics
- delayed retry scheduling
- graceful shutdown
- metrics

Phase 3:

- NATS backend or second backend abstraction
- OpenTelemetry export
- leader election / recovery loop
- circuit breaker / advanced backpressure

---

## Suggested Crate Structure

Use a workspace if helpful.

Possible structure:

- `taskq-core`
  - task model
  - traits
  - errors
  - retry logic
  - shared types

- `taskq-runtime`
  - worker pool
  - dispatcher
  - shutdown
  - backpressure

- `taskq-backend-memory`
  - in-memory backend for tests/dev

- `taskq-backend-redis`
  - Redis backend

- `taskq-backend-nats`
  - NATS backend (later or optional)

- `taskq-observability`
  - tracing setup
  - metrics
  - OpenTelemetry integration

- `taskq-cli` or `examples/`
  - demo producer/worker
  - local playground

Keep the design practical. A monorepo workspace is fine, but avoid unnecessary complexity.

---

## Domain Model Expectations

Core entities:

### Task

A task should likely include:

- id
- queue name
- payload
- headers / metadata
- attempts
- max_attempts
- scheduled_at
- visibility_deadline / lease info
- created_at
- trace context if relevant

### Delivery / Reservation

Represent the act of leasing a task to a worker separately from the task itself where useful.

### Retry decision

Retry behavior should be explicit and testable.

---

## Reliability Expectations

Target semantics:

- at-least-once delivery
- task handlers should be written as idempotent where possible
- retries should use exponential backoff with optional jitter
- poison messages should end up in a DLQ
- worker crashes should not lose leased tasks forever

Make failure modes explicit in code and documentation.

---

## Observability Expectations

Instrument important operations with:

- tracing spans
- structured fields: task_id, queue, attempt, worker_id, backend
- metrics for:
  - enqueued tasks
  - processing latency
  - success/failure counts
  - retries
  - DLQ count
  - in-flight tasks
  - queue depth if available

When adding instrumentation, keep it useful and not noisy.

---

## Backpressure Expectations

Backpressure is a first-class concern.

Prefer bounded queues and concurrency limits.
The system should not accept infinite in-memory growth.

Examples:

- bounded producer channels
- max in-flight tasks per worker pool
- backend poll throttling
- configurable reserve batch size
- rejection or delay behavior when overloaded

Document how backpressure works.

---

## Code Quality Expectations

- idiomatic Rust
- meaningful names
- avoid unnecessary cloning
- derive traits appropriately
- use `thiserror` or equivalent for errors
- prefer integration tests for end-to-end behavior
- add doc comments on public interfaces
- explain non-obvious concurrency logic
- do not silence warnings casually

---

## Testing Expectations

Add tests for:

- retry backoff calculation
- task success path
- task failure then retry
- max attempts to DLQ
- graceful shutdown behavior
- visibility timeout / lease recovery
- bounded concurrency / backpressure behavior

Prefer deterministic tests.
Use Tokio test utilities where helpful.

---

## When Assisting

When helping on this repo:

1. first explain the architecture impact of a change
2. then propose the minimal clean implementation
3. then show code
4. call out risks and follow-up improvements

When generating code:

- keep code compile-oriented
- mention missing imports if partial
- do not invent APIs without stating assumptions
- prefer complete small increments over huge speculative scaffolding

When reviewing code:

- identify correctness risks
- identify async/concurrency risks
- identify observability gaps
- identify simplifications

---

## Non-Goals For Now

Do not start with:

- exactly-once delivery
- full web dashboard
- Kubernetes deployment
- multi-region consensus
- authentication / multi-tenancy
- too many backends at once

Focus on building one strong, coherent, explainable system.

---

## Deliverable Standard

This repository is an exploration of distributed task queue design in Rust.

The goal is to experiment with real systems concerns such as:

- async runtime behavior
- backpressure
- failure handling and retries
- delivery guarantees
- observability
- clean system boundaries

The project should remain understandable and well documented so that other engineers can follow the design decisions and tradeoffs.

Each major feature should improve one of the following:

- system reliability
- architectural clarity
- observability
- learning value

When possible, design decisions should be documented along with alternatives and tradeoffs.

## Development Philosophy

This project should evolve incrementally.

Prefer:

- small working implementations
- clear iteration
- documented tradeoffs

Avoid:

- speculative architecture
- premature optimization
- adding complexity before the core system works end-to-end

Features should be introduced only after the previous layer is stable.

## Design Notes

Interesting design decisions and tradeoffs should be documented in `docs/`.

Examples:

- retry strategy choices
- Redis data modeling decisions
- backpressure mechanisms
- worker concurrency model
