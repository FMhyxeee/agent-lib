use tokio::time::{timeout, Duration};

use agent_lib::protocol::Op;
use agent_lib::session::Session;

#[tokio::test]
async fn session_emits_events_for_start_turn() {
    let (_session, handle) = Session::new(4);

    handle
        .submit(Op::StartTurn {
            prompt: "hello".to_string(),
            context: Default::default(),
        })
        .await
        .unwrap();

    let first = timeout(Duration::from_secs(1), handle.next_event())
        .await
        .unwrap()
        .unwrap();
    let second = timeout(Duration::from_secs(1), handle.next_event())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(first, agent_lib::protocol::Event::TurnStarted { .. }));
    assert!(matches!(
        second,
        agent_lib::protocol::Event::ModelComplete { .. }
    ));
}
