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
