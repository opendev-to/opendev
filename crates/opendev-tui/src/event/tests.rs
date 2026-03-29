use super::*;
use std::time::Duration;

#[test]
fn test_event_handler_creation() {
    let handler = EventHandler::new(Duration::from_millis(250));
    let _sender = handler.sender();
}

#[tokio::test]
async fn test_sender_delivers_events() {
    let mut handler = EventHandler::new(Duration::from_millis(250));
    let tx = handler.sender();
    tx.send(AppEvent::Tick).unwrap();
    let event = handler.next().await.unwrap();
    assert!(matches!(event, AppEvent::Tick));
}

#[tokio::test]
async fn test_quit_event() {
    let mut handler = EventHandler::new(Duration::from_millis(250));
    let tx = handler.sender();
    tx.send(AppEvent::Quit).unwrap();
    let event = handler.next().await.unwrap();
    assert!(matches!(event, AppEvent::Quit));
}
