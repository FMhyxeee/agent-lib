use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use agent_lib::protocol::{Event, EventQueue, Op, SubmissionQueue};

#[tokio::test]
async fn submission_queue_sends_ops() {
    let (sender, mut receiver) = mpsc::channel(1);
    let queue = SubmissionQueue::new(sender);

    queue.submit(Op::Interrupt).await.unwrap();
    let op = receiver.recv().await.unwrap();

    assert!(matches!(op, Op::Interrupt));
}

#[tokio::test]
async fn event_queue_streams_events() {
    let (sender, queue) = EventQueue::new(1);
    let mut stream = queue.stream();

    sender
        .send(Event::TurnStarted {
            turn_id: "turn-1".to_string(),
        })
        .await
        .unwrap();

    let event = stream.next().await.unwrap();
    assert!(matches!(event, Event::TurnStarted { .. }));
}
