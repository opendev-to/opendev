use super::*;

#[test]
fn test_tool_permission_is_allowed() {
    let perm = ToolPermission {
        enabled: true,
        always_allow: false,
        deny_patterns: vec!["rm -rf /".to_string()],
    };
    assert!(perm.is_allowed("ls -la"));
    assert!(!perm.is_allowed("rm -rf /"));

    let disabled = ToolPermission {
        enabled: false,
        ..Default::default()
    };
    assert!(!disabled.is_allowed("anything"));

    let allow_all = ToolPermission {
        enabled: true,
        always_allow: true,
        deny_patterns: vec![".*".to_string()],
    };
    assert!(allow_all.is_allowed("anything"));
}
