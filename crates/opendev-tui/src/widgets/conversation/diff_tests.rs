use super::*;

#[test]
fn test_reformat_summary() {
    assert_eq!(
        reformat_summary("Edited file.rs: 1 replacement(s), 2 addition(s) and 1 removal(s)"),
        "Added 2 lines, removed 1 line"
    );
    assert_eq!(
        reformat_summary("Edited file.rs: 1 replacement(s), 0 addition(s) and 3 removal(s)"),
        "removed 3 lines"
    );
    assert_eq!(
        reformat_summary("Some unknown format"),
        "Some unknown format"
    );
}

#[test]
fn test_parse_unified_diff_line_numbers() {
    let result_lines = vec![
        "Edited file.rs: 1 replacement(s), 2 addition(s) and 1 removal(s)".to_string(),
        "--- a/file.rs".to_string(),
        "+++ b/file.rs".to_string(),
        "@@ -201,5 +201,6 @@".to_string(),
        " context line".to_string(),
        "-old line".to_string(),
        "+new line 1".to_string(),
        "+new line 2".to_string(),
        " trailing context".to_string(),
    ];
    let (summary, entries) = parse_unified_diff(&result_lines);

    assert_eq!(summary, "Added 2 lines, removed 1 line");
    assert_eq!(entries.len(), 5);

    // Context line at 201
    assert_eq!(entries[0].entry_type, DiffEntryType::Ctx);
    assert_eq!(entries[0].line_no, Some(201));
    assert_eq!(entries[0].content, "context line");

    // Deletion at 202
    assert_eq!(entries[1].entry_type, DiffEntryType::Del);
    assert_eq!(entries[1].line_no, Some(202));
    assert_eq!(entries[1].content, "old line");

    // Additions at 202, 203
    assert_eq!(entries[2].entry_type, DiffEntryType::Add);
    assert_eq!(entries[2].line_no, Some(202));
    assert_eq!(entries[3].entry_type, DiffEntryType::Add);
    assert_eq!(entries[3].line_no, Some(203));

    // Trailing context at 203 (old), 204 (new)
    assert_eq!(entries[4].entry_type, DiffEntryType::Ctx);
    assert_eq!(entries[4].line_no, Some(203));
}
