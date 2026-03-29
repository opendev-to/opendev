//! Sandbox session — the core iterative loop.
//!
//! Orchestrates: LLM call -> Python execution -> output capture -> repeat until FINAL().

use tracing::{debug, info};

use crate::callback::CallbackServer;
use crate::errors::{Result, SandboxError};
use crate::models::{SandboxContext, SandboxRequest, SandboxResult};
use crate::parser::{self, TerminalValue};
use crate::prompts;
use crate::sandbox::MicroSandbox;
use opendev_models::config::SandboxConfig;

/// A sandbox session that runs the iterative LLM-code-execute cycle.
pub struct SandboxSession<'a> {
    sandbox: &'a MicroSandbox,
    callback: &'a CallbackServer,
    config: &'a SandboxConfig,
    model: String,
}

/// Message role for the sandbox conversation.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used when LLM call is wired.
struct Message {
    role: String,
    content: String,
}

impl<'a> SandboxSession<'a> {
    pub fn new(
        sandbox: &'a MicroSandbox,
        callback: &'a CallbackServer,
        config: &'a SandboxConfig,
        model: impl Into<String>,
    ) -> Self {
        Self {
            sandbox,
            callback,
            config,
            model: model.into(),
        }
    }

    /// Run the sandbox loop for the given request.
    pub async fn run(&self, request: &SandboxRequest) -> Result<SandboxResult> {
        // 1. Inject context into sandbox.
        let mut context_vars: Vec<(String, usize)> = Vec::new();

        if let Some(ref ctx) = request.context {
            match ctx {
                SandboxContext::Text { name, content } => {
                    self.sandbox.inject_variable(name, content).await?;
                    context_vars.push((name.clone(), content.len()));
                }
                SandboxContext::File { path } => {
                    let content = tokio::fs::read_to_string(path)
                        .await
                        .map_err(|e| SandboxError::Other(format!("Failed to read file: {e}")))?;
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("context")
                        .to_string();
                    self.sandbox.inject_variable(&name, &content).await?;
                    context_vars.push((name, content.len()));
                }
            }
        }

        // 2. Inject lm_query stubs.
        let stubs = prompts::build_lm_query_stubs(self.callback.port());
        self.sandbox.run_code(&stubs).await?;

        // 3. Build initial messages.
        let system_prompt = prompts::build_system_prompt(&context_vars, self.config.max_iterations);
        let mut messages = vec![
            Message {
                role: "system".to_string(),
                content: system_prompt,
            },
            Message {
                role: "user".to_string(),
                content: request.query.clone(),
            },
        ];

        // 4. Sandbox loop.
        for iteration in 0..self.config.max_iterations {
            debug!(iteration, "Sandbox loop iteration");

            // Call LLM.
            let response = self.call_llm(&messages).await?;

            // Check for terminal answer.
            match parser::extract_terminal(&response) {
                TerminalValue::Final(answer) => {
                    info!(iteration, "Sandbox completed with FINAL()");
                    return Ok(SandboxResult {
                        answer,
                        iterations: iteration + 1,
                        lm_query_count: self.callback.query_count(),
                        total_tokens: 0, // TODO: track from LLM responses
                    });
                }
                TerminalValue::FinalVar(var_name) => {
                    info!(iteration, var = %var_name, "Sandbox completed with FINAL_VAR()");
                    let value = self.sandbox.run_code(&format!("print({var_name})")).await?;
                    return Ok(SandboxResult {
                        answer: value.trim().to_string(),
                        iterations: iteration + 1,
                        lm_query_count: self.callback.query_count(),
                        total_tokens: 0,
                    });
                }
                TerminalValue::None => {}
            }

            // Execute the LLM response as Python code in the sandbox.
            let exec_result = match self.sandbox.run_code(&response).await {
                Ok(output) => {
                    if output.len() > self.config.output_max_chars {
                        let truncated = &output[..self.config.output_max_chars];
                        format!(
                            "{truncated}\n\n[Output truncated: {} chars total, showing first {}]",
                            output.len(),
                            self.config.output_max_chars,
                        )
                    } else {
                        output
                    }
                }
                Err(e) => format!("Error: {e}"),
            };

            // Append to conversation history.
            messages.push(Message {
                role: "assistant".to_string(),
                content: response,
            });
            messages.push(Message {
                role: "user".to_string(),
                content: if exec_result.is_empty() {
                    "[No output]".to_string()
                } else {
                    exec_result
                },
            });
        }

        Err(SandboxError::MaxIterations {
            max_iterations: self.config.max_iterations,
        })
    }

    /// Call the LLM with the current conversation messages.
    async fn call_llm(&self, _messages: &[Message]) -> Result<String> {
        // TODO: Build chat completions payload, call AdaptedClient::post_json().
        let _ = &self.model;
        Err(SandboxError::LlmCall(
            "LLM call not yet wired — pending AdaptedClient integration".to_string(),
        ))
    }
}
