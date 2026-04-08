//! invoke_skill tool — loads skill content into conversation context on demand.
//!
//! Supports listing available skills, loading by name (with namespace),
//! session-scoped deduplication, forked execution as sub-agents, conditional
//! activation via path globs, visibility controls, embedded hooks, and
//! runtime variable substitution.

mod arguments;
mod mcp;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use opendev_mcp::McpManager;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use opendev_agents::skills::{SkillContext, SkillLoader};

use arguments::expand_skill_arguments;

/// Tool that loads skill content into the conversation context.
///
/// Skills are markdown files with YAML frontmatter discovered from:
/// - `<project>/.opendev/skills/` (highest priority)
/// - `~/.opendev/skills/`
/// - Built-in skills embedded in the binary
///
/// Supports:
/// - **Conditional activation**: Skills with `paths` globs are hidden until
///   matching files are touched.
/// - **Forked execution**: Skills with `context: fork` execute as isolated
///   sub-agents with separate token budgets and tool restrictions.
/// - **Visibility controls**: `disable-model-invocation` hides from model,
///   `user-invocable` controls slash command access.
/// - **Embedded hooks**: Skills can define lifecycle hooks.
/// - **Runtime variables**: `${SKILL_DIR}`, `${SESSION_ID}`, `${WORKING_DIR}`.
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
        "Skill"
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
        ctx: &ToolContext,
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
                    "explore",
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

        // Check visibility: model cannot invoke skills with disable_model_invocation.
        let is_user_invocation =
            ctx.values.get("invocation_source").and_then(|v| v.as_str()) == Some("user");
        if skill.metadata.disable_model_invocation && !is_user_invocation {
            return ToolResult::fail(format!(
                "Skill '{}' can only be invoked via slash command (e.g. /{}). \
                 It has disable-model-invocation: true.",
                skill.metadata.name, skill.metadata.name
            ));
        }

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
        let mut skill_content = if !arguments.is_empty() {
            expand_skill_arguments(&skill.content, arguments)
        } else {
            skill.content.clone()
        };

        // Apply runtime variable substitution.
        let session_id = ctx.session_id.as_deref().unwrap_or("unknown");
        let runtime_vars = SkillLoader::build_runtime_variables(&skill, session_id);
        skill_content = SkillLoader::expand_variables(&skill_content, &runtime_vars);
        skill_content = SkillLoader::expand_dollar_variables(&skill_content, &runtime_vars);

        // Build base metadata.
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

        // Handle forked execution context.
        if skill.metadata.context == SkillContext::Fork {
            meta.insert("skill_fork".into(), serde_json::json!(true));
            meta.insert(
                "skill_effort".into(),
                serde_json::json!(skill.metadata.effort.reasoning_effort()),
            );
            meta.insert(
                "skill_max_steps".into(),
                serde_json::json!(skill.metadata.effort.max_steps()),
            );
            if !skill.metadata.allowed_tools.is_empty() {
                meta.insert(
                    "skill_allowed_tools".into(),
                    serde_json::json!(skill.metadata.allowed_tools),
                );
            }

            let token_estimate = skill_content.len() / 4;
            meta.insert("token_estimate".into(), serde_json::json!(token_estimate));

            // For forked skills, return the content as a sub-agent prompt.
            // The ReAct loop will detect `skill_fork: true` in metadata and
            // spawn a sub-agent instead of injecting the content inline.
            let output = format!(
                "Forked skill: {} (effort: {}, max_steps: {})\n\n\
                 <skill_fork name=\"{}\" effort=\"{}\" max_steps=\"{}\">\n{}\n</skill_fork>\n\n\
                 This skill should be executed as an isolated sub-agent. \
                 Use spawn_subagent with the content above as the prompt.",
                skill.metadata.name,
                skill.metadata.effort.reasoning_effort(),
                skill.metadata.effort.max_steps(),
                skill.metadata.name,
                skill.metadata.effort.reasoning_effort(),
                skill.metadata.effort.max_steps(),
                skill_content,
            );

            return ToolResult::ok_with_metadata(output, meta);
        }

        // Inline execution: inject content into conversation.
        let token_estimate = skill_content.len() / 4;
        meta.insert("token_estimate".into(), serde_json::json!(token_estimate));

        // Include embedded hooks in metadata for the runtime to register.
        if !skill.metadata.hooks.is_empty() {
            let hooks_json: Vec<serde_json::Value> = skill
                .metadata
                .hooks
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "event": h.event,
                        "matcher": h.matcher,
                        "command": h.command,
                    })
                })
                .collect();
            meta.insert("skill_hooks".into(), serde_json::json!(hooks_json));
        }

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
mod tests;
