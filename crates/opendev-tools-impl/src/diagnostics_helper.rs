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
#[path = "diagnostics_helper_tests.rs"]
mod tests;
