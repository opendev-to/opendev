use super::*;

#[test]
fn test_sanitize_mcp_name_simple() {
    assert_eq!(sanitize_mcp_name("my-server"), "my-server");
    assert_eq!(sanitize_mcp_name("my_tool"), "my_tool");
    assert_eq!(sanitize_mcp_name("tool123"), "tool123");
}

#[test]
fn test_sanitize_mcp_name_special_chars() {
    assert_eq!(sanitize_mcp_name("tool/name"), "tool_name");
    assert_eq!(sanitize_mcp_name("my.server"), "my_server");
    assert_eq!(sanitize_mcp_name("ns:tool"), "ns_tool");
    assert_eq!(sanitize_mcp_name("a b c"), "a_b_c");
}

#[test]
fn test_sanitize_mcp_name_preserves_valid() {
    assert_eq!(sanitize_mcp_name("ABC-xyz_123"), "ABC-xyz_123");
    assert_eq!(sanitize_mcp_name(""), "");
}
