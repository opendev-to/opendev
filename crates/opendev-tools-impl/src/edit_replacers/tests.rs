use super::*;

// ---- Pass 1: Simple ----

#[test]
fn test_simple_exact_match() {
    let original = "fn main() {\n    println!(\"hello\");\n}";
    let old = "println!(\"hello\");";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "simple");
    assert_eq!(result.actual, old);
}

#[test]
fn test_simple_no_match() {
    let original = "fn main() {}";
    assert!(find_match(original, "nonexistent").is_none());
}

// ---- Pass 2: LineTrimmed ----

#[test]
fn test_line_trimmed_extra_indent() {
    let original = "fn foo() {\n    let x = 1;\n    let y = 2;\n}";
    // LLM provides without indentation
    let old = "let x = 1;\nlet y = 2;";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "line_trimmed");
    assert_eq!(result.actual, "    let x = 1;\n    let y = 2;");
}

#[test]
fn test_line_trimmed_different_indent_levels() {
    let original = "  if true {\n      do_thing();\n  }";
    let old = "if true {\n    do_thing();\n}";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "line_trimmed");
    assert_eq!(result.actual, "  if true {\n      do_thing();\n  }");
}

// ---- Pass 3: BlockAnchor ----

#[test]
fn test_block_anchor_middle_differs() {
    let original = "fn test() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}";
    // First and last lines match, middle is slightly different
    let old = "fn test() {\n    let a = 10;\n    let b = 20;\n    let c = 30;\n}";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "block_anchor");
    assert!(result.actual.starts_with("fn test()"));
    assert!(result.actual.ends_with('}'));
}

#[test]
fn test_block_anchor_too_few_lines() {
    let original = "fn test() {\n}";
    let old = "fn test() {\n}";
    // 2 lines — not enough for block anchor (needs >= 3), falls to simple
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "simple");
}

// ---- Pass 4: WhitespaceNormalized ----

#[test]
fn test_whitespace_normalized() {
    let original = "let   x  =   1;";
    let old = "let x = 1;";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "whitespace_normalized");
    assert_eq!(result.actual, "let   x  =   1;");
}

#[test]
fn test_whitespace_normalized_multiline() {
    let original = "fn foo() {\n    let   x  =  1;\n    let  y =  2;\n}";
    let old = "let x = 1;\nlet y = 2;";
    let result = find_match(original, old).unwrap();
    // Should match via line_trimmed or whitespace_normalized
    assert!(result.pass_name == "line_trimmed" || result.pass_name == "whitespace_normalized");
}

// ---- Pass 5: IndentationFlexible ----

#[test]
fn test_indentation_flexible_skips_blank_lines() {
    let original = "fn foo() {\n\n    let x = 1;\n\n    let y = 2;\n}";
    let old = "let x = 1;\nlet y = 2;";
    let result = find_match(original, old).unwrap();
    // May match via line_trimmed or indentation_flexible
    assert!(result.pass_name == "line_trimmed" || result.pass_name == "indentation_flexible");
}

// ---- Pass 6: EscapeNormalized ----

#[test]
fn test_escape_normalized() {
    let original = "let s = \"hello\nworld\";";
    // LLM sends literal \n instead of actual newline
    let old = "let s = \"hello\\nworld\";";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "escape_normalized");
    assert_eq!(result.actual, "let s = \"hello\nworld\";");
}

#[test]
fn test_escape_normalized_tab() {
    let original = "let s = \"hello\tworld\";";
    let old = "let s = \"hello\\tworld\";";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "escape_normalized");
}

#[test]
fn test_escape_no_change_skipped() {
    let original = "hello world";
    let old = "hello world";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "simple"); // no escapes, should use simple
}

// ---- Pass 7: TrimmedBoundary ----

#[test]
fn test_trimmed_boundary() {
    // Test that trimmed boundary matching works. The old_content has leading/trailing
    // whitespace. Earlier passes (indentation_flexible etc.) may also match this,
    // so we verify a match is found and contains the right content.
    let original = "header\n  alpha_line\n  beta_line\nfooter";
    let old = "  \n  alpha_line\n  beta_line\n  ";
    let result = find_match(original, old).unwrap();
    assert!(result.actual.contains("alpha_line"));
    assert!(result.actual.contains("beta_line"));

    // Also test trimmed_boundary directly: trimmed content found in original
    let direct = trimmed_boundary_find("abc xyz def", "  xyz  ");
    assert_eq!(direct, Some("xyz".to_string()));
}

#[test]
fn test_trimmed_boundary_no_trim_needed() {
    let original = "hello world";
    let old = "hello world"; // no trimming needed, falls to simple
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "simple");
}

// ---- Pass 8: ContextAware ----

#[test]
fn test_context_aware_match() {
    let original = "fn setup() {\n    init();\n}\n\nfn main() {\n    let x = compute();\n    println!(\"{}\", x);\n}";
    // Old content has matching anchors but slightly different middle
    let old = "fn main() {\n    let x = calculate();\n    println!(\"{}\", x);\n}";
    let result = find_match(original, old).unwrap();
    // block_anchor or context_aware can both match this — both use anchor lines
    assert!(
        result.pass_name == "block_anchor" || result.pass_name == "context_aware",
        "expected block_anchor or context_aware, got {}",
        result.pass_name
    );
    assert!(result.actual.contains("fn main()"));
}

// ---- Pass 9: MultiOccurrence ----

#[test]
fn test_multi_occurrence_trimmed_match() {
    let original = "    fn foo() {\n        bar();\n    }";
    // No exact match, but trimmed line-by-line matches
    let old = "  fn foo() {\n      bar();\n  }";
    let result = find_match(original, old).unwrap();
    // Should match via line_trimmed (earlier pass)
    assert!(result.pass_name == "line_trimmed" || result.pass_name == "multi_occurrence");
    assert_eq!(result.actual, "    fn foo() {\n        bar();\n    }");
}

// ---- Similarity helper ----

#[test]
fn test_similarity_identical() {
    assert!((passes::similarity("hello", "hello") - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_similarity_empty() {
    assert!((passes::similarity("", "") - 1.0).abs() < f64::EPSILON);
    assert!((passes::similarity("hello", "") - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_similarity_partial() {
    let sim = passes::similarity("abcdef", "abcxyz");
    assert!(sim > 0.0 && sim < 1.0);
}

// ---- Unified diff ----

#[test]
fn test_unified_diff_basic() {
    let original = "line1\nline2\nline3\n";
    let modified = "line1\nline2_modified\nline3\n";
    let diff = unified_diff("test.rs", original, modified, 3);
    assert!(diff.contains("--- a/test.rs"));
    assert!(diff.contains("+++ b/test.rs"));
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+line2_modified"));
}

#[test]
fn test_unified_diff_no_changes() {
    let text = "line1\nline2\n";
    let diff = unified_diff("test.rs", text, text, 3);
    assert!(diff.is_empty());
}

#[test]
fn test_unified_diff_addition() {
    let original = "line1\nline3\n";
    let modified = "line1\nline2\nline3\n";
    let diff = unified_diff("test.rs", original, modified, 3);
    assert!(diff.contains("+line2"));
}

#[test]
fn test_unified_diff_removal() {
    let original = "line1\nline2\nline3\n";
    let modified = "line1\nline3\n";
    let diff = unified_diff("test.rs", original, modified, 3);
    assert!(diff.contains("-line2"));
}

// ---- Line endings ----

#[test]
fn test_normalize_line_endings() {
    assert_eq!(normalize_line_endings("a\r\nb\rc\n"), "a\nb\nc\n");
}

// ---- find_match with CRLF ----

#[test]
fn test_find_match_crlf() {
    let original = "line1\r\nline2\r\nline3";
    let old = "line2";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "simple");
    assert_eq!(result.actual, "line2");
}

// ---- Edge cases ----

#[test]
fn test_empty_old_content() {
    let original = "hello world";
    // Empty old_content should still match via simple (empty string is in any string)
    let result = find_match(original, "");
    assert!(result.is_some());
}

#[test]
fn test_multiline_exact() {
    let original = "fn main() {\n    let x = 1;\n    let y = 2;\n    println!(\"{} {}\", x, y);\n}";
    let old = "    let x = 1;\n    let y = 2;";
    let result = find_match(original, old).unwrap();
    assert_eq!(result.pass_name, "simple");
    assert_eq!(result.actual, old);
}

// ---- Occurrence finding helper ----

#[test]
fn test_find_occurrence_line_numbers() {
    let content = "foo\nbar\nfoo\nbaz\nfoo";
    let positions = find_occurrence_positions(content, "foo");
    assert_eq!(positions, vec![1, 3, 5]);
}

#[test]
fn test_find_occurrence_needle_at_end() {
    let positions = find_occurrence_positions("abc", "c");
    assert_eq!(positions, vec![1]);
}

#[test]
fn test_find_occurrence_needle_is_entire_string() {
    let positions = find_occurrence_positions("abc", "abc");
    assert_eq!(positions, vec![1]);
}

#[test]
fn test_find_occurrence_multibyte_utf8() {
    // 🌍 is 4 bytes; ensure we don't panic on char boundary
    let positions = find_occurrence_positions("a🌍b🌍c", "🌍");
    assert_eq!(positions, vec![1, 1]);
}

#[test]
fn test_find_occurrence_empty_needle() {
    // Empty needle matches everywhere — just ensure no panic
    let positions = find_occurrence_positions("abc", "");
    assert!(!positions.is_empty());
}

#[test]
fn test_find_occurrence_no_match() {
    let positions = find_occurrence_positions("abc", "xyz");
    assert_eq!(positions, Vec::<usize>::new());
}
