//! AstGrepTool — structural code search using ast-grep.

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use super::types::AstGrepArgs;
use crate::dir_hints::list_available_dirs;
use crate::path_utils::{resolve_dir_path, validate_path_access};

/// Tool for structural code search using ast-grep.
#[derive(Debug)]
pub struct AstGrepTool;

#[async_trait::async_trait]
impl BaseTool for AstGrepTool {
    fn name(&self) -> &str {
        "ast_grep"
    }

    fn description(&self) -> &str {
        "Search code structurally using AST patterns via ast-grep. \
         Use $VAR wildcards for structural matching (e.g., \"$A && $A()\"). \
         $$$VAR matches multiple nodes (e.g., \"fn $NAME() { $$$BODY }\")."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "AST pattern with $VAR wildcards for structural matching"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to working directory)"
                },
                "lang": {
                    "type": "string",
                    "description": "Language hint (e.g., 'rust', 'javascript', 'python'). Auto-detected from file extension if not specified."
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N matches"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let ast_args = match AstGrepArgs::from_map(&args) {
            Ok(a) => a,
            Err(e) => return ToolResult::fail(e),
        };

        let search_path = ast_args
            .path
            .as_deref()
            .map(|p| resolve_dir_path(p, &ctx.working_dir))
            .unwrap_or_else(|| ctx.working_dir.clone());

        if let Err(msg) = validate_path_access(&search_path, &ctx.working_dir) {
            return ToolResult::fail(msg);
        }

        if !search_path.exists() {
            let available = list_available_dirs(&ctx.working_dir);
            return ToolResult::fail(format!(
                "Path not found: {}\n\nAvailable directories in working dir ({}):\n{}",
                search_path.display(),
                ctx.working_dir.display(),
                available
            ));
        }

        self.run_ast_grep(&ast_args, &search_path).await
    }
}
