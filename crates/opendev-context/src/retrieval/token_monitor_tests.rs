use super::*;

#[test]
fn test_count_tokens_empty() {
    let monitor = ContextTokenMonitor::new();
    assert_eq!(monitor.count_tokens(""), 0);
}

#[test]
fn test_count_tokens_short() {
    let monitor = ContextTokenMonitor::new();
    // 11 chars / 4 = 2
    assert_eq!(monitor.count_tokens("hello world"), 2);
}

#[test]
fn test_count_tokens_longer() {
    let monitor = ContextTokenMonitor::new();
    let text = "a".repeat(100);
    assert_eq!(monitor.count_tokens(&text), 25);
}

#[test]
fn test_default_trait() {
    let monitor = ContextTokenMonitor::default();
    assert_eq!(monitor.count_tokens("test"), 1);
}
