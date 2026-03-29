use regex::Regex;
use std::sync::LazyLock;

/// Parsed terminal value from an LLM response.
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalValue {
    /// `FINAL("answer")` — literal answer string.
    Final(String),
    /// `FINAL_VAR(variable_name)` — name of a variable to read from sandbox.
    FinalVar(String),
    /// No terminal marker found.
    None,
}

// Pre-compiled regex patterns for FINAL extraction (ordered by specificity).
static RE_FINAL_TRIPLE_DOUBLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"FINAL\(\s*"""([\s\S]+?)"""\s*\)"#).unwrap());
static RE_FINAL_TRIPLE_SINGLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"FINAL\(\s*'''([\s\S]+?)'''\s*\)").unwrap());
static RE_FINAL_DOUBLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"FINAL\(\s*"([\s\S]+?)"\s*\)"#).unwrap());
static RE_FINAL_SINGLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"FINAL\(\s*'([\s\S]+?)'\s*\)").unwrap());
static RE_FINAL_VAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"FINAL_VAR\(\s*(\w+)\s*\)").unwrap());

/// Quick check whether the response might contain a terminal marker.
pub fn is_terminal(response: &str) -> bool {
    response.contains("FINAL(") || response.contains("FINAL_VAR(")
}

/// Extract a terminal value from an LLM response.
///
/// Checks for `FINAL("answer")` first (various quoting styles), then
/// `FINAL_VAR(variable_name)`. Returns `TerminalValue::None` if neither found.
pub fn extract_terminal(response: &str) -> TerminalValue {
    if !is_terminal(response) {
        return TerminalValue::None;
    }

    // Try FINAL() patterns in order: triple-quoted first (most specific).
    for re in [
        &*RE_FINAL_TRIPLE_DOUBLE,
        &*RE_FINAL_TRIPLE_SINGLE,
        &*RE_FINAL_DOUBLE,
        &*RE_FINAL_SINGLE,
    ] {
        if let Some(caps) = re.captures(response)
            && let Some(m) = caps.get(1)
        {
            return TerminalValue::Final(m.as_str().trim().to_string());
        }
    }

    // Try FINAL_VAR(variable_name).
    if let Some(caps) = RE_FINAL_VAR.captures(response)
        && let Some(m) = caps.get(1)
    {
        return TerminalValue::FinalVar(m.as_str().to_string());
    }

    TerminalValue::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_final_double_quotes() {
        assert_eq!(
            extract_terminal(r#"FINAL("hello world")"#),
            TerminalValue::Final("hello world".to_string()),
        );
    }

    #[test]
    fn test_extract_final_single_quotes() {
        assert_eq!(
            extract_terminal("FINAL('the answer is 42')"),
            TerminalValue::Final("the answer is 42".to_string()),
        );
    }

    #[test]
    fn test_extract_final_triple_double_quotes() {
        let input = r#"FINAL("""
This is a
multiline answer.
""")"#;
        assert_eq!(
            extract_terminal(input),
            TerminalValue::Final("This is a\nmultiline answer.".to_string()),
        );
    }

    #[test]
    fn test_extract_final_triple_single_quotes() {
        let input = "FINAL('''multi\nline''')";
        assert_eq!(
            extract_terminal(input),
            TerminalValue::Final("multi\nline".to_string()),
        );
    }

    #[test]
    fn test_extract_final_var() {
        assert_eq!(
            extract_terminal("FINAL_VAR(result_df)"),
            TerminalValue::FinalVar("result_df".to_string()),
        );
    }

    #[test]
    fn test_extract_final_var_with_whitespace() {
        assert_eq!(
            extract_terminal("FINAL_VAR(  my_var  )"),
            TerminalValue::FinalVar("my_var".to_string()),
        );
    }

    #[test]
    fn test_no_terminal() {
        assert_eq!(extract_terminal("print('hello')"), TerminalValue::None,);
    }

    #[test]
    fn test_is_terminal_true() {
        assert!(is_terminal("some code\nFINAL('done')"));
        assert!(is_terminal("FINAL_VAR(x)"));
    }

    #[test]
    fn test_is_terminal_false() {
        assert!(!is_terminal("print('hello')"));
        assert!(!is_terminal("FINALE('not this')"));
    }

    #[test]
    fn test_final_surrounded_by_code() {
        let input = r#"
# After analysis
results = analyze(data)
FINAL("The answer is 42")
# This shouldn't matter
"#;
        assert_eq!(
            extract_terminal(input),
            TerminalValue::Final("The answer is 42".to_string()),
        );
    }

    #[test]
    fn test_final_takes_precedence_over_final_var() {
        // When both present, FINAL() is checked first.
        let input = r#"FINAL("direct") and FINAL_VAR(indirect)"#;
        assert_eq!(
            extract_terminal(input),
            TerminalValue::Final("direct".to_string()),
        );
    }
}
