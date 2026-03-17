//! invoke_skill tool — loads skill content into conversation context on demand.
//!
//! Supports listing available skills, loading by name (with namespace),
//! and session-scoped deduplication to avoid re-loading the same skill.

mod arguments;
mod mcp;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use opendev_mcp::McpManager;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use opendev_agents::skills::SkillLoader;

use arguments::expand_skill_arguments;

/// Tool that loads skill content into the conversation context.
///
/// Skills are markdown files with YAML frontmatter discovered from:
/// - `<project>/.opendev/skills/` (highest priority)
/// - `~/.opendev/skills/`
/// - Built-in skills embedded in the binary
///
/// Also surfaces MCP prompts as invokable commands using `server:prompt` syntax.
pub struct InvokeSkillTool {
    skill_loader: Arc<Mutex<SkillLoader>>,
    /// Tracks which skills have been invoked this session to avoid re-loading.
    pub(crate) invoked_skills: Mutex<HashSet<String>>,
    /// Optional MCP manager for surfacing MCP prompts.
    mcp_manager: Option<Arc<McpManager>>,
}

impl std::fmt::Debug for InvokeSkillTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvokeSkillTool")
            .field("skill_loader", &"<SkillLoader>")
            .field("invoked_skills", &self.invoked_skills)
            .field(
                "mcp_manager",
                &self.mcp_manager.as_ref().map(|_| "<McpManager>"),
            )
            .finish()
    }
}

impl InvokeSkillTool {
    /// Create a new invoke_skill tool with a shared skill loader.
    pub fn new(skill_loader: Arc<Mutex<SkillLoader>>) -> Self {
        Self {
            skill_loader,
            invoked_skills: Mutex::new(HashSet::new()),
            mcp_manager: None,
        }
    }

    /// Create a new invoke_skill tool with MCP prompt support.
    pub fn with_mcp(skill_loader: Arc<Mutex<SkillLoader>>, mcp_manager: Arc<McpManager>) -> Self {
        Self {
            skill_loader,
            invoked_skills: Mutex::new(HashSet::new()),
            mcp_manager: Some(mcp_manager),
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for InvokeSkillTool {
    fn name(&self) -> &str {
        "invoke_skill"
    }

    fn description(&self) -> &str {
        "Load a predefined skill that the user explicitly mentioned by name (e.g. /commit, review-pr). \
         Do NOT use for general tasks like code exploration, summarization, or analysis — \
         use spawn_subagent for those instead. Only call when the user's message contains a skill name. \
         Call without skill_name to list available skills."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "description": "Name of the skill to load (e.g. 'commit', 'git:rebase'). Omit to list all available skills."
                },
                "arguments": {
                    "type": "string",
                    "description": "Optional arguments to pass to the skill template. \
                                    Skills can use $ARGUMENTS for the full string, or $1, $2, etc. for positional args."
                }
            },
            "required": []
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let skill_name = args
            .get("skill_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        enum SkillLookup {
            ListOnly(Vec<String>),
            SubagentRedirect(String),
            Found(Box<opendev_agents::skills::LoadedSkill>),
            NotFound(Vec<String>),
        }

        let lookup = {
            let mut loader = match self.skill_loader.lock() {
                Ok(l) => l,
                Err(_) => return ToolResult::fail("Failed to acquire skill loader lock"),
            };

            if skill_name.is_empty() {
                SkillLookup::ListOnly(loader.get_skill_names())
            } else {
                let subagent_types = [
                    "code-explorer",
                    "code_explorer",
                    "planner",
                    "general",
                    "build",
                    "ask-user",
                    "ask_user",
                ];
                let normalized = skill_name.to_lowercase();
                if subagent_types.iter().any(|t| normalized == *t) {
                    SkillLookup::SubagentRedirect(normalized.replace('-', "_"))
                } else {
                    match loader.load_skill(skill_name) {
                        Some(s) => SkillLookup::Found(Box::new(s)),
                        None => SkillLookup::NotFound(loader.get_skill_names()),
                    }
                }
            }
        };

        match lookup {
            SkillLookup::ListOnly(names) => {
                let mut sorted = names;
                sorted.sort();

                let mcp_prompts = if let Some(ref mgr) = self.mcp_manager {
                    mgr.list_prompts().await
                } else {
                    vec![]
                };

                if sorted.is_empty() && mcp_prompts.is_empty() {
                    return ToolResult::ok("No skills available.");
                }

                let mut output = String::new();
                if !sorted.is_empty() {
                    output.push_str(&format!("Available skills: {}", sorted.join(", ")));
                }
                if !mcp_prompts.is_empty() {
                    if !output.is_empty() {
                        output.push_str("\n\n");
                    }
                    output.push_str("MCP prompts:\n");
                    for p in &mcp_prompts {
                        let args_str = if p.arguments.is_empty() {
                            String::new()
                        } else {
                            format!(" (args: {})", p.arguments.join(", "))
                        };
                        output.push_str(&format!(
                            "  {} — {}{}\n",
                            p.command, p.description, args_str
                        ));
                    }
                }
                return ToolResult::ok(output.trim_end().to_string());
            }
            SkillLookup::SubagentRedirect(agent_type) => {
                return ToolResult::fail(format!(
                    "'{skill_name}' is a subagent type, not a skill. \
                     Use spawn_subagent with agent_type: \"{agent_type}\" instead. \
                     invoke_skill is only for loading predefined skills the user mentioned by name \
                     (e.g. /commit, /review-pr)."
                ));
            }
            SkillLookup::NotFound(skill_names) => {
                if let Some(ref mgr) = self.mcp_manager
                    && let Some(result) = self.try_mcp_prompt(mgr, skill_name, &args).await
                {
                    return result;
                }

                let available = if skill_names.is_empty() {
                    "None".to_string()
                } else {
                    let mut sorted = skill_names;
                    sorted.sort();
                    sorted.join(", ")
                };
                return ToolResult::fail(format!(
                    "Skill not found: '{skill_name}'. \
                     invoke_skill is only for predefined skills the user mentioned by name \
                     (e.g. /commit, /review-pr). For general tasks like code exploration or \
                     summarization, use spawn_subagent instead. Available skills: {available}"
                ));
            }
            SkillLookup::Found(_) => {}
        }

        let SkillLookup::Found(skill) = lookup else {
            unreachable!()
        };

        // Dedup: if already invoked this session, return a short reminder.
        if let Ok(mut invoked) = self.invoked_skills.lock() {
            if invoked.contains(skill_name) {
                let mut meta = HashMap::new();
                meta.insert(
                    "skill_name".to_string(),
                    serde_json::json!(skill.metadata.name),
                );
                meta.insert(
                    "skill_namespace".to_string(),
                    serde_json::json!(skill.metadata.namespace),
                );
                return ToolResult::ok_with_metadata(
                    format!(
                        "Skill '{}' is already loaded in this conversation. \
                         Refer to the skill content above and proceed with the next action step — \
                         do not invoke this skill again.",
                        skill.metadata.name
                    ),
                    meta,
                );
            }
            invoked.insert(skill_name.to_string());
        }

        // Apply argument substitution if provided.
        let arguments = args
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let skill_content = if !arguments.is_empty() {
            expand_skill_arguments(&skill.content, arguments)
        } else {
            skill.content.clone()
        };

        // Return the full skill content with metadata.
        let mut meta = HashMap::new();
        meta.insert(
            "skill_name".to_string(),
            serde_json::json!(skill.metadata.name),
        );
        meta.insert(
            "skill_namespace".to_string(),
            serde_json::json!(skill.metadata.namespace),
        );
        if let Some(ref model) = skill.metadata.model {
            meta.insert("skill_model".to_string(), serde_json::json!(model));
        }
        if let Some(ref agent) = skill.metadata.agent {
            meta.insert("skill_agent".to_string(), serde_json::json!(agent));
        }

        let token_estimate = skill_content.len() / 4;
        meta.insert("token_estimate".into(), serde_json::json!(token_estimate));

        let mut output = format!(
            "Loaded skill: {} (~{} tokens)\n\n<skill_content name=\"{}\">\n{}\n</skill_content>",
            skill.metadata.name, token_estimate, skill.metadata.name, skill_content
        );

        if !skill.companion_files.is_empty() {
            let base_dir = skill
                .metadata
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            output.push_str("\n\n<skill_files>\n");
            for cf in &skill.companion_files {
                output.push_str(&format!("<file>{}</file>\n", cf.path.display()));
            }
            output.push_str("</skill_files>\n");
            output.push_str(&format!(
                "\nBase directory for this skill: {}\n\
                 Relative paths in this skill are relative to this base directory.\n\
                 Note: file list is sampled.",
                base_dir
            ));
        }

        ToolResult::ok_with_metadata(output, meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_loader(skill_dir: Option<&std::path::Path>) -> Arc<Mutex<SkillLoader>> {
        let dirs = match skill_dir {
            Some(d) => vec![d.to_path_buf()],
            None => vec![],
        };
        let mut loader = SkillLoader::new(dirs);
        loader.discover_skills();
        Arc::new(Mutex::new(loader))
    }

    #[tokio::test]
    async fn test_list_skills_no_arg() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Available skills:"));
        assert!(output.contains("commit"));
    }

    #[tokio::test]
    async fn test_list_skills_empty_string() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!(""));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("Available skills:"));
    }

    #[tokio::test]
    async fn test_load_builtin_skill() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("commit"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Loaded skill: commit"));
        assert!(output.contains("Git Commit"));
        assert_eq!(result.metadata.get("skill_name").unwrap(), "commit");
        assert_eq!(result.metadata.get("skill_namespace").unwrap(), "default");
    }

    #[tokio::test]
    async fn test_skill_not_found() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert(
            "skill_name".to_string(),
            serde_json::json!("nonexistent-skill-xyz"),
        );
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let error = result.error.unwrap();
        assert!(error.contains("Skill not found: 'nonexistent-skill-xyz'"));
        assert!(error.contains("Available skills:"));
    }

    #[tokio::test]
    async fn test_subagent_type_redirects_to_spawn_subagent() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        for name in &["code-explorer", "code_explorer", "planner", "ask_user"] {
            let mut args = HashMap::new();
            args.insert("skill_name".to_string(), serde_json::json!(name));
            let result = tool.execute(args, &ctx).await;
            assert!(!result.success, "Should fail for subagent type '{name}'");
            let error = result.error.unwrap();
            assert!(error.contains("subagent type, not a skill"));
            assert!(error.contains("spawn_subagent"));
        }
    }

    #[tokio::test]
    async fn test_dedup_second_invoke_returns_reminder() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("commit"));
        let result1 = tool.execute(args.clone(), &ctx).await;
        assert!(result1.success);
        assert!(result1.output.unwrap().contains("Loaded skill: commit"));
        let result2 = tool.execute(args, &ctx).await;
        assert!(result2.success);
        let output2 = result2.output.unwrap();
        assert!(output2.contains("already loaded"));
        assert!(output2.contains("do not invoke this skill again"));
    }

    #[tokio::test]
    async fn test_load_filesystem_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy instructions\nnamespace: ops\n---\n\n# Deploy\nStep 1: push.\n",
        ).unwrap();
        let loader = create_test_loader(Some(&skill_dir));
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("deploy"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Loaded skill: deploy"));
        assert!(output.contains("Step 1: push."));
        assert_eq!(result.metadata.get("skill_namespace").unwrap(), "ops");
    }

    #[tokio::test]
    async fn test_load_directory_skill_with_companions() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        let sub_dir = skill_dir.join("testing");
        fs::create_dir_all(&sub_dir).unwrap();
        fs::write(
            sub_dir.join("SKILL.md"),
            "---\nname: testing\ndescription: Testing patterns\n---\n\n# Testing\nTest content.\n",
        )
        .unwrap();
        fs::write(sub_dir.join("helpers.sh"), "#!/bin/bash\necho test").unwrap();
        fs::write(sub_dir.join("config.json"), r#"{"key": "val"}"#).unwrap();
        let loader = create_test_loader(Some(&skill_dir));
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("testing"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Loaded skill: testing"));
        assert!(output.contains("<skill_files>"));
        assert!(output.contains("helpers.sh"));
        assert!(output.contains("config.json"));
        assert!(output.contains("Base directory for this skill:"));
    }

    #[tokio::test]
    async fn test_invoke_skill_with_arguments() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("greet.md"),
            "---\nname: greet\ndescription: Greet someone\n---\n\nHello $1, welcome to $2!\n",
        )
        .unwrap();
        let loader = create_test_loader(Some(&skill_dir));
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("greet"));
        args.insert("arguments".to_string(), serde_json::json!("Alice OpenDev"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(
            result
                .output
                .unwrap()
                .contains("Hello Alice, welcome to OpenDev!")
        );
    }

    #[tokio::test]
    async fn test_invoke_skill_with_model_override() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("fast-lint.md"),
            "---\nname: fast-lint\ndescription: Fast lint\nmodel: gpt-4o-mini\n---\n\n# Lint\nDo fast linting.\n",
        ).unwrap();
        let loader = create_test_loader(Some(&skill_dir));
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("fast-lint"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert_eq!(result.metadata.get("skill_model").unwrap(), "gpt-4o-mini");
    }

    #[tokio::test]
    async fn test_invoke_skill_without_model_no_metadata() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("commit"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.metadata.get("skill_model").is_none());
    }

    #[tokio::test]
    async fn test_invoke_skill_with_agent_override() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("deploy.md"),
            "---\nname: deploy\ndescription: Deploy\nagent: devops\n---\n\n# Deploy\nDeploy steps.\n",
        ).unwrap();
        let loader = create_test_loader(Some(&skill_dir));
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("deploy"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert_eq!(result.metadata.get("skill_agent").unwrap(), "devops");
    }

    #[tokio::test]
    async fn test_invoke_skill_without_agent_no_metadata() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("commit"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.metadata.get("skill_agent").is_none());
    }

    #[tokio::test]
    async fn test_skill_output_wrapped_in_xml() {
        let loader = create_test_loader(None);
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("commit"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("<skill_content name=\"commit\">"));
        assert!(output.contains("</skill_content>"));
        assert!(result.metadata.get("token_estimate").is_some());
    }

    #[tokio::test]
    async fn test_load_namespaced_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("rebase.md"),
            "---\nname: rebase\ndescription: Git rebase\nnamespace: git\n---\n\n# Rebase\n",
        )
        .unwrap();
        let loader = create_test_loader(Some(&skill_dir));
        let tool = InvokeSkillTool::new(loader);
        let ctx = ToolContext::new("/tmp/test");
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), serde_json::json!("git:rebase"));
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("Loaded skill: rebase"));
        assert_eq!(result.metadata.get("skill_namespace").unwrap(), "git");
    }
}
