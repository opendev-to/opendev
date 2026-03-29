use super::*;

#[test]
fn test_format_tokens() {
    assert_eq!(StatusBarWidget::format_tokens(500), "500");
    assert_eq!(StatusBarWidget::format_tokens(1_500), "1.5k");
    assert_eq!(StatusBarWidget::format_tokens(1_500_000), "1.5M");
}

#[test]
fn test_shorten_display() {
    let ps = crate::formatters::PathShortener::default();
    assert_eq!(ps.shorten_display("/home/user"), "/home/user");
    assert_eq!(ps.shorten_display("/a/b/c/d/myapp"), "…/d/myapp");
}

#[test]
fn test_status_bar_creation() {
    let _widget = StatusBarWidget::new(
        "claude-sonnet-4",
        "/home/user/project",
        Some("main"),
        5000,
        200_000,
        OperationMode::Normal,
    )
    .autonomy(AutonomyLevel::Manual)
    .context_usage_pct(25.0)
    .session_cost(0.05)
    .mcp_status(Some((2, 3)), false)
    .background_tasks(1);
}

#[test]
fn test_autonomy_display() {
    assert_eq!(AutonomyLevel::Manual.to_string(), "Manual");
    assert_eq!(AutonomyLevel::SemiAuto.to_string(), "Semi-Auto");
    assert_eq!(AutonomyLevel::Auto.to_string(), "Auto");
}
