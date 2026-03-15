//! Post-edit diagnostic collection helper.
//!
//! After file modifications (edit, write, patch), this module queries
//! the optional `DiagnosticProvider` on the `ToolContext` and formats
//! any errors/warnings into a string that gets appended to the tool output.
//! This gives the LLM immediate feedback about introduced errors.

use std::path::Path;

use opendev_tools_core::ToolContext;

/// Maximum number of diagnostics to include per file.
const MAX_DIAGNOSTICS_PER_FILE: usize = 20;

/// Maximum number of extra project files to report diagnostics for.
const MAX_PROJECT_DIAGNOSTIC_FILES: usize = 5;

/// Collect LSP diagnostics for a file after modification.
///
/// Returns a formatted string suitable for appending to tool output,
/// or `None` if no diagnostics are available or no provider is configured.
pub async fn collect_post_edit_diagnostics(ctx: &ToolContext, file_path: &Path) -> Option<String> {
    let provider = ctx.diagnostic_provider.as_ref()?;

    // Query diagnostics for the edited file — errors and warnings only (severity ≤ 2).
    let diagnostics = provider
        .diagnostics_for_file(file_path, 2, MAX_DIAGNOSTICS_PER_FILE)
        .await;

    if diagnostics.is_empty() {
        return None;
    }

    let mut output = String::new();

    // Count errors vs warnings
    let error_count = diagnostics.iter().filter(|d| d.severity == 1).count();
    let warning_count = diagnostics.iter().filter(|d| d.severity == 2).count();

    output.push_str("\nLSP diagnostics detected after edit:");
    output.push_str(&format!("\n<diagnostics file=\"{}\">", file_path.display()));

    for diag in &diagnostics {
        output.push('\n');
        output.push_str(&diag.pretty());
    }

    output.push_str("\n</diagnostics>");

    if error_count > 0 {
        output.push_str(&format!(
            "\n\n{error_count} error(s) and {warning_count} warning(s) found. Please fix the errors."
        ));
    }

    Some(output)
}

/// Collect diagnostics for multiple files (used by patch tool).
///
/// Returns formatted diagnostic output for all modified files,
/// limited to `MAX_PROJECT_DIAGNOSTIC_FILES` files.
pub async fn collect_multi_file_diagnostics(
    ctx: &ToolContext,
    file_paths: &[&Path],
) -> Option<String> {
    let provider = ctx.diagnostic_provider.as_ref()?;

    let mut output = String::new();
    let mut files_with_diags = 0;

    for &file_path in file_paths.iter().take(MAX_PROJECT_DIAGNOSTIC_FILES + 1) {
        let diagnostics = provider
            .diagnostics_for_file(file_path, 2, MAX_DIAGNOSTICS_PER_FILE)
            .await;

        if diagnostics.is_empty() {
            continue;
        }

        files_with_diags += 1;
        if files_with_diags > MAX_PROJECT_DIAGNOSTIC_FILES {
            output.push_str(&format!(
                "\n... and more files with diagnostics (showing first {MAX_PROJECT_DIAGNOSTIC_FILES})"
            ));
            break;
        }

        output.push_str(&format!("\n<diagnostics file=\"{}\">", file_path.display()));

        for diag in &diagnostics {
            output.push('\n');
            output.push_str(&diag.pretty());
        }

        output.push_str("\n</diagnostics>");
    }

    if output.is_empty() {
        return None;
    }

    let mut result = String::from("\nLSP diagnostics detected after edit:");
    result.push_str(&output);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendev_tools_core::{DiagnosticProvider, FileDiagnostic, ToolContext};
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Mock diagnostic provider for testing.
    #[derive(Debug)]
    struct MockDiagnosticProvider {
        diagnostics: Vec<(PathBuf, Vec<FileDiagnostic>)>,
    }

    #[async_trait::async_trait]
    impl DiagnosticProvider for MockDiagnosticProvider {
        async fn diagnostics_for_file(
            &self,
            file_path: &Path,
            max_severity: u32,
            max_count: usize,
        ) -> Vec<FileDiagnostic> {
            self.diagnostics
                .iter()
                .find(|(p, _)| p == file_path)
                .map(|(_, diags)| {
                    diags
                        .iter()
                        .filter(|d| d.severity <= max_severity)
                        .take(max_count)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    #[tokio::test]
    async fn test_no_provider_returns_none() {
        let ctx = ToolContext::new("/tmp");
        let result = collect_post_edit_diagnostics(&ctx, Path::new("/tmp/test.rs")).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_no_diagnostics_returns_none() {
        let provider = Arc::new(MockDiagnosticProvider {
            diagnostics: vec![],
        });
        let ctx = ToolContext::new("/tmp").with_diagnostic_provider(provider);
        let result = collect_post_edit_diagnostics(&ctx, Path::new("/tmp/test.rs")).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_errors_reported() {
        let file = PathBuf::from("/tmp/test.rs");
        let provider = Arc::new(MockDiagnosticProvider {
            diagnostics: vec![(
                file.clone(),
                vec![
                    FileDiagnostic {
                        line: 10,
                        column: 5,
                        severity: 1,
                        message: "expected `;`".to_string(),
                    },
                    FileDiagnostic {
                        line: 15,
                        column: 1,
                        severity: 2,
                        message: "unused variable `x`".to_string(),
                    },
                ],
            )],
        });
        let ctx = ToolContext::new("/tmp").with_diagnostic_provider(provider);

        let result = collect_post_edit_diagnostics(&ctx, &file).await;
        let output = result.unwrap();

        assert!(output.contains("LSP diagnostics detected"));
        assert!(output.contains("ERROR [10:5] expected `;`"));
        assert!(output.contains("WARN [15:1] unused variable `x`"));
        assert!(output.contains("1 error(s) and 1 warning(s)"));
        assert!(output.contains("Please fix the errors"));
    }

    #[tokio::test]
    async fn test_warnings_only_no_fix_prompt() {
        let file = PathBuf::from("/tmp/test.rs");
        let provider = Arc::new(MockDiagnosticProvider {
            diagnostics: vec![(
                file.clone(),
                vec![FileDiagnostic {
                    line: 5,
                    column: 1,
                    severity: 2,
                    message: "unused import".to_string(),
                }],
            )],
        });
        let ctx = ToolContext::new("/tmp").with_diagnostic_provider(provider);

        let result = collect_post_edit_diagnostics(&ctx, &file).await;
        let output = result.unwrap();

        assert!(output.contains("WARN [5:1] unused import"));
        // No "Please fix" prompt since there are no errors
        assert!(!output.contains("Please fix"));
    }

    #[tokio::test]
    async fn test_multi_file_diagnostics() {
        let file_a = PathBuf::from("/tmp/a.rs");
        let file_b = PathBuf::from("/tmp/b.rs");
        let file_c = PathBuf::from("/tmp/c.rs");

        let provider = Arc::new(MockDiagnosticProvider {
            diagnostics: vec![
                (
                    file_a.clone(),
                    vec![FileDiagnostic {
                        line: 1,
                        column: 1,
                        severity: 1,
                        message: "error in a".to_string(),
                    }],
                ),
                (
                    file_b.clone(),
                    vec![FileDiagnostic {
                        line: 2,
                        column: 1,
                        severity: 1,
                        message: "error in b".to_string(),
                    }],
                ),
                // c has no diagnostics
            ],
        });
        let ctx = ToolContext::new("/tmp").with_diagnostic_provider(provider);

        let paths: Vec<&Path> = vec![file_a.as_path(), file_b.as_path(), file_c.as_path()];
        let result = collect_multi_file_diagnostics(&ctx, &paths).await;
        let output = result.unwrap();

        assert!(output.contains("error in a"));
        assert!(output.contains("error in b"));
        assert!(output.contains("<diagnostics file=\"/tmp/a.rs\">"));
        assert!(output.contains("<diagnostics file=\"/tmp/b.rs\">"));
    }

    #[tokio::test]
    async fn test_multi_file_no_diagnostics() {
        let provider = Arc::new(MockDiagnosticProvider {
            diagnostics: vec![],
        });
        let ctx = ToolContext::new("/tmp").with_diagnostic_provider(provider);

        let file = PathBuf::from("/tmp/test.rs");
        let paths: Vec<&Path> = vec![file.as_path()];
        let result = collect_multi_file_diagnostics(&ctx, &paths).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_diagnostic_pretty_formatting() {
        let d = FileDiagnostic {
            line: 42,
            column: 15,
            severity: 1,
            message: "type mismatch".to_string(),
        };
        assert_eq!(d.pretty(), "ERROR [42:15] type mismatch");

        let d2 = FileDiagnostic {
            line: 1,
            column: 1,
            severity: 2,
            message: "unused".to_string(),
        };
        assert_eq!(d2.pretty(), "WARN [1:1] unused");

        let d3 = FileDiagnostic {
            line: 1,
            column: 1,
            severity: 3,
            message: "info".to_string(),
        };
        assert_eq!(d3.pretty(), "INFO [1:1] info");

        let d4 = FileDiagnostic {
            line: 1,
            column: 1,
            severity: 4,
            message: "hint".to_string(),
        };
        assert_eq!(d4.pretty(), "HINT [1:1] hint");
    }
}
