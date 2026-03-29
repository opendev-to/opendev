use super::*;

#[tokio::test]
async fn test_ask_user_roundtrip() {
    let (tx, mut rx) = ask_user_channel();
    let (resp_tx, resp_rx) = oneshot::channel();

    tx.send(AskUserRequest {
        question: "What language?".into(),
        options: vec!["Rust".into(), "Python".into()],
        default: Some("Rust".into()),
        response_tx: resp_tx,
    })
    .unwrap();

    let req = rx.recv().await.unwrap();
    assert!(req.question.contains("language"));
    assert_eq!(req.options.len(), 2);

    req.response_tx.send("Rust".into()).unwrap();

    let answer = resp_rx.await.unwrap();
    assert_eq!(answer, "Rust");
}
