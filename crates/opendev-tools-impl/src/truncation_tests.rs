use super::*;

#[test]
fn test_no_truncation_small_output() {
    let text = "line 1\nline 2\nline 3";
    let result = truncate_output(text, None, None, TruncateDirection::Head);
    assert!(!result.truncated);
    assert_eq!(result.content, text);
    assert!(result.output_path.is_none());
}

#[test]
fn test_truncation_by_lines() {
    let text = (0..10)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_output(&text, Some(3), Some(100_000), TruncateDirection::Head);
    assert!(result.truncated);
    assert!(result.content.contains("line 0"));
    assert!(result.content.contains("line 2"));
    assert!(result.content.contains("truncated"));
    assert!(result.output_path.is_some());

    // Clean up.
    if let Some(p) = &result.output_path {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn test_truncation_by_bytes() {
    // Each line is ~10 bytes. 50 bytes limit should keep ~5 lines.
    let text = (0..20)
        .map(|i| format!("line {i:04}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_output(&text, Some(100), Some(50), TruncateDirection::Head);
    assert!(result.truncated);
    assert!(result.content.contains("bytes truncated"));
    assert!(result.output_path.is_some());

    if let Some(p) = &result.output_path {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn test_truncation_tail_direction() {
    let text = (0..10)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_output(&text, Some(3), Some(100_000), TruncateDirection::Tail);
    assert!(result.truncated);
    // Tail keeps the last 3 lines.
    assert!(result.content.contains("line 9"));
    assert!(result.content.contains("line 8"));
    assert!(result.content.contains("line 7"));
    // First lines should not be in the preview portion.
    let parts: Vec<&str> = result.content.splitn(2, "truncated").collect();
    let after_hint = parts.get(1).unwrap_or(&"");
    assert!(after_hint.contains("line 9"));

    if let Some(p) = &result.output_path {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn test_overflow_file_contains_full_output() {
    let text = (0..100)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_output(&text, Some(5), Some(100_000), TruncateDirection::Head);
    assert!(result.truncated);
    let path = result.output_path.unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(saved, text);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_output_path_in_hint() {
    let text = (0..100)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_output(&text, Some(5), Some(100_000), TruncateDirection::Head);
    assert!(result.truncated);
    let path = result.output_path.as_ref().unwrap();
    assert!(
        result.content.contains(&path.display().to_string()),
        "Hint should contain the file path"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_cleanup_keeps_recent_files() {
    // Files just created should NOT be cleaned up.
    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path();

    let recent = dir_path.join("tool_9999_abcd1234");
    std::fs::write(&recent, "data").unwrap();
    let non_tool = dir_path.join("other_file.txt");
    std::fs::write(&non_tool, "data").unwrap();

    // Run cleanup logic against this directory (inline since cleanup_old_files
    // uses the real output_dir).
    let entries = std::fs::read_dir(dir_path).unwrap();
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(RETENTION_SECS))
        .unwrap();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("tool_") {
            continue;
        }
        if let Ok(meta) = entry.metadata()
            && let Ok(mtime) = meta.modified()
            && mtime < cutoff
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    assert!(recent.exists(), "Recent tool file should be kept");
    assert!(non_tool.exists(), "Non-tool file should be untouched");
}
