use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_find_symbol_range_rust_fn() {
    let code = "use std::io;\n\nfn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";
    let lines: Vec<&str> = code.lines().collect();
    let range = find_symbol_range(&lines, "hello").unwrap();
    assert_eq!(range.start_line, 2);
    assert_eq!(range.end_line, 4);
}

#[test]
fn test_find_symbol_range_pub_fn() {
    let code = "pub fn my_func(x: i32) -> i32 {\n    x + 1\n}\n";
    let lines: Vec<&str> = code.lines().collect();
    let range = find_symbol_range(&lines, "my_func").unwrap();
    assert_eq!(range.start_line, 0);
    assert_eq!(range.end_line, 2);
}

#[test]
fn test_find_symbol_range_struct() {
    let code = "pub struct Foo {\n    bar: i32,\n}\n";
    let lines: Vec<&str> = code.lines().collect();
    let range = find_symbol_range(&lines, "Foo").unwrap();
    assert_eq!(range.start_line, 0);
    assert_eq!(range.end_line, 2);
}

#[test]
fn test_find_symbol_range_python_def() {
    let code = "def greet(name):\n    print(f\"Hello {name}\")\n    return True\n\ndef other():\n    pass\n";
    let lines: Vec<&str> = code.lines().collect();
    let range = find_symbol_range(&lines, "greet").unwrap();
    assert_eq!(range.start_line, 0);
    assert_eq!(range.end_line, 2);
}

#[test]
fn test_find_symbol_range_not_found() {
    let code = "fn hello() {}\n";
    let lines: Vec<&str> = code.lines().collect();
    assert!(find_symbol_range(&lines, "nonexistent").is_none());
}

#[test]
fn test_insert_before() {
    let code = "fn a() {}\n\nfn b() {\n    1\n}\n";
    let lines: Vec<&str> = code.lines().collect();
    let range = find_symbol_range(&lines, "b").unwrap();
    let result = insert_content(
        code,
        &lines,
        "// inserted\n",
        &range,
        InsertPosition::Before,
    );
    assert!(result.contains("// inserted\nfn b()"));
}

#[test]
fn test_insert_after() {
    let code = "fn a() {\n    1\n}\n\nfn b() {}\n";
    let lines: Vec<&str> = code.lines().collect();
    let range = find_symbol_range(&lines, "a").unwrap();
    let result = insert_content(code, &lines, "// inserted\n", &range, InsertPosition::After);
    // After fn a's closing brace, the inserted content should appear
    assert!(result.contains("}\n// inserted\n"));
}
