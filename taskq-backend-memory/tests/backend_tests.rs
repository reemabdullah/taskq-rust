use chrono::{Duration, Utc};
use taskq_backend_memory::{InMemoryBackend, InMemoryBackendConfig};
use taskq_core::task::{Task, TaskStatus};
use taskq_core::QueueBackend;

#[tokio::test]
async fn enqueue_and_reserve_round_trip() {
    let backend = InMemoryBackend::new();
    let task = Task::new("jobs", b"hello".to_vec(), 3);
    let id = backend.enqueue(task).await.unwrap();

    let reserved = backend.reserve("jobs").await.unwrap().unwrap();
    assert_eq!(reserved.id, id);
    assert_eq!(reserved.status, TaskStatus::Active);
    assert_eq!(reserved.payload, b"hello");
}

#[tokio::test]
async fn reserve_empty_queue_returns_none() {
    let backend = InMemoryBackend::new();
    let result = backend.reserve("empty").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn ack_completes_task() {
    let backend = InMemoryBackend::new();
    let task = Task::new("jobs", vec![], 3);
    let id = backend.enqueue(task).await.unwrap();

    let _ = backend.reserve("jobs").await.unwrap().unwrap();
    backend.ack(&id).await.unwrap();

    let stored = backend.get_task(&id).unwrap();
    assert_eq!(stored.status, TaskStatus::Completed);

    // Should not be reservable again.
    assert!(backend.reserve("jobs").await.unwrap().is_none());
}

#[tokio::test]
async fn nack_re_enqueues_task() {
    let backend = InMemoryBackend::new();
    let task = Task::new("jobs", vec![], 3);
    let id = backend.enqueue(task).await.unwrap();

    let _ = backend.reserve("jobs").await.unwrap().unwrap();
    backend.nack(&id, None).await.unwrap();

    let stored = backend.get_task(&id).unwrap();
    assert_eq!(stored.status, TaskStatus::Pending);
    assert_eq!(stored.attempts, 1);

    // Should be reservable again immediately.
    let reserved = backend.reserve("jobs").await.unwrap().unwrap();
    assert_eq!(reserved.id, id);
}

#[tokio::test]
async fn nack_with_delay_hides_task() {
    let backend = InMemoryBackend::new();
    let task = Task::new("jobs", vec![], 3);
    let id = backend.enqueue(task).await.unwrap();

    let _ = backend.reserve("jobs").await.unwrap().unwrap();

    // Schedule retry far in the future.
    let future = Utc::now() + Duration::hours(1);
    backend.nack(&id, Some(future)).await.unwrap();

    // Should not be visible yet.
    assert!(backend.reserve("jobs").await.unwrap().is_none());
}

#[tokio::test]
async fn nack_with_past_delay_makes_task_immediately_available() {
    let backend = InMemoryBackend::new();
    let task = Task::new("jobs", vec![], 3);
    let id = backend.enqueue(task).await.unwrap();

    let _ = backend.reserve("jobs").await.unwrap().unwrap();

    let past = Utc::now() - Duration::seconds(10);
    backend.nack(&id, Some(past)).await.unwrap();

    let reserved = backend.reserve("jobs").await.unwrap().unwrap();
    assert_eq!(reserved.id, id);
}

#[tokio::test]
async fn move_to_dlq_marks_dead_lettered() {
    let backend = InMemoryBackend::new();
    let task = Task::new("jobs", vec![], 3);
    let id = backend.enqueue(task).await.unwrap();

    let _ = backend.reserve("jobs").await.unwrap().unwrap();
    backend.move_to_dlq(&id).await.unwrap();

    let stored = backend.get_task(&id).unwrap();
    assert_eq!(stored.status, TaskStatus::DeadLettered);
    assert_eq!(backend.dlq_depth("jobs"), 1);

    // Should not be reservable.
    assert!(backend.reserve("jobs").await.unwrap().is_none());
}

#[tokio::test]
async fn queue_full_rejection() {
    let backend = InMemoryBackend::with_config(InMemoryBackendConfig {
        max_queue_size: Some(2),
    });

    let t1 = Task::new("jobs", vec![], 3);
    let t2 = Task::new("jobs", vec![], 3);
    let t3 = Task::new("jobs", vec![], 3);

    backend.enqueue(t1).await.unwrap();
    backend.enqueue(t2).await.unwrap();

    let err = backend.enqueue(t3).await.unwrap_err();
    assert!(matches!(err, taskq_core::QueueError::QueueFull));
}

#[tokio::test]
async fn fifo_ordering() {
    let backend = InMemoryBackend::new();

    let t1 = Task::new("jobs", b"first".to_vec(), 3);
    let t2 = Task::new("jobs", b"second".to_vec(), 3);
    let t3 = Task::new("jobs", b"third".to_vec(), 3);

    let id1 = backend.enqueue(t1).await.unwrap();
    let id2 = backend.enqueue(t2).await.unwrap();
    let id3 = backend.enqueue(t3).await.unwrap();

    let r1 = backend.reserve("jobs").await.unwrap().unwrap();
    let r2 = backend.reserve("jobs").await.unwrap().unwrap();
    let r3 = backend.reserve("jobs").await.unwrap().unwrap();

    assert_eq!(r1.id, id1);
    assert_eq!(r2.id, id2);
    assert_eq!(r3.id, id3);
}

#[tokio::test]
async fn multiple_queues_are_independent() {
    let backend = InMemoryBackend::new();

    let t1 = Task::new("queue-a", b"a".to_vec(), 3);
    let t2 = Task::new("queue-b", b"b".to_vec(), 3);

    let id_a = backend.enqueue(t1).await.unwrap();
    let id_b = backend.enqueue(t2).await.unwrap();

    let reserved_a = backend.reserve("queue-a").await.unwrap().unwrap();
    assert_eq!(reserved_a.id, id_a);

    let reserved_b = backend.reserve("queue-b").await.unwrap().unwrap();
    assert_eq!(reserved_b.id, id_b);

    // queue-a is now empty.
    assert!(backend.reserve("queue-a").await.unwrap().is_none());
}

#[tokio::test]
async fn ack_unknown_task_returns_not_found() {
    let backend = InMemoryBackend::new();
    let fake_id = taskq_core::TaskId::new();
    let err = backend.ack(&fake_id).await.unwrap_err();
    assert!(matches!(err, taskq_core::QueueError::TaskNotFound(_)));
}
