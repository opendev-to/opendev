use super::*;

#[test]
fn test_new_token_not_requested() {
    let token = InterruptToken::new();
    assert!(!token.is_requested());
    assert!(!token.should_interrupt());
}

#[test]
fn test_request_sets_flag() {
    let token = InterruptToken::new();
    token.request();
    assert!(token.is_requested());
}

#[test]
fn test_throw_if_requested_ok() {
    let token = InterruptToken::new();
    assert!(token.throw_if_requested().is_ok());
}

#[test]
fn test_throw_if_requested_err() {
    let token = InterruptToken::new();
    token.request();
    assert!(token.throw_if_requested().is_err());
}

#[test]
fn test_clone_shares_state() {
    let token = InterruptToken::new();
    let clone = token.clone();
    assert!(!clone.is_requested());
    token.request();
    assert!(clone.is_requested());
}

#[test]
fn test_force_interrupt() {
    let token = InterruptToken::new();
    token.force_interrupt();
    assert!(token.is_requested());
}

#[test]
fn test_reset() {
    let token = InterruptToken::new();
    token.request();
    assert!(token.is_requested());
    token.reset();
    assert!(!token.is_requested());
}

#[test]
fn test_request_background_sets_flag() {
    let token = InterruptToken::new();
    assert!(!token.is_background_requested());
    token.request_background();
    assert!(token.is_background_requested());
}

#[test]
fn test_request_background_cancels_but_no_hard_flag() {
    let token = InterruptToken::new();
    token.request_background();
    assert!(token.is_background_requested());
    // Should cancel the CancellationToken (to interrupt in-flight ops)
    // but NOT set the hard interrupt flag
    assert!(!token.is_requested());
    assert!(token.cancellation_token().is_cancelled());
}

#[test]
fn test_background_clone_shares_state() {
    let token = InterruptToken::new();
    let clone = token.clone();
    token.request_background();
    assert!(clone.is_background_requested());
}

#[test]
fn test_request_idempotent() {
    let token = InterruptToken::new();
    token.request();
    token.request();
    token.request();
    assert!(token.is_requested());
}

#[test]
fn test_debug_format() {
    let token = InterruptToken::new();
    let debug = format!("{:?}", token);
    assert!(debug.contains("InterruptToken"));
    assert!(debug.contains("false"));
}

#[tokio::test]
async fn test_cancelled_future() {
    let token = InterruptToken::new();
    let token2 = token.clone();

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        token2.request();
    });

    // Should resolve once token is requested
    token.cancelled().await;
    assert!(token.is_requested());
}

#[tokio::test]
async fn test_select_with_cancelled() {
    let token = InterruptToken::new();
    let token2 = token.clone();

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        token2.request();
    });

    let result = tokio::select! {
        _ = token.cancelled() => "interrupted",
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => "timeout",
    };

    assert_eq!(result, "interrupted");
}

#[test]
fn test_child_token() {
    let token = InterruptToken::new();
    let child = token.child_token();
    assert!(!child.is_cancelled());
    token.request();
    assert!(child.is_cancelled());
}
