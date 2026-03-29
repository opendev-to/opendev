/// Format a number with commas for readability (e.g. 50000 -> "50,000").
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Build the REALM system prompt for the root LLM.
///
/// `context_vars` is a list of `(variable_name, char_count)` for each injected context.
pub fn build_system_prompt(context_vars: &[(String, usize)], max_iterations: u32) -> String {
    let vars_section = if context_vars.is_empty() {
        "No context variables loaded.".to_string()
    } else {
        context_vars
            .iter()
            .map(|(name, size)| {
                let formatted_size = format_number(*size);
                format!("- `{name}` (str, {formatted_size} characters)")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"You are a computational reasoning agent.
You solve problems by writing Python code that executes in a persistent sandbox.

## Available Variables
{vars_section}

## Available Functions
- `lm_query(prompt, context="")` -> str: Ask an LLM a question. Pass relevant text as context. Use for reasoning, classification, summarization, or extraction over text chunks.
- `parallel_lm_query(queries)` -> list[str]: Query the LLM with multiple (prompt, context) pairs in parallel. Each item is a dict with "prompt" and "context" keys.
- `FINAL(answer)`: Call when you have the final answer. The argument is returned as the result.
- `FINAL_VAR(variable_name)`: Return the value of a Python variable as the final answer.

## Rules
1. Write Python code. It will be executed and you will see the stdout output.
2. Variables persist between executions — build on previous results incrementally.
3. You have **{max_iterations} iterations** maximum. Be efficient.
4. **Always call FINAL() or FINAL_VAR() when done.** Do not just print the answer.
5. Use `lm_query()` for tasks requiring language understanding (summarization, classification, extraction).
6. Use `parallel_lm_query()` to batch multiple LLM queries efficiently.
7. For large data: chunk and process incrementally. Do not try to fit everything in one LLM call.
8. Standard library modules are available: `re`, `json`, `math`, `collections`, `itertools`, `string`, `datetime`.
9. Do NOT guess. Search and verify before calling FINAL().
10. If code produces an error, read the error message and write corrected code."#,
    )
}

/// Generate Python code that defines the `lm_query` and `parallel_lm_query`
/// functions, pointing at the callback server on `callback_port`.
pub fn build_lm_query_stubs(callback_port: u16) -> String {
    format!(
        r#"
import urllib.request
import json as _json

def lm_query(prompt, context=""):
    """Query an LLM. Returns the response string."""
    _data = _json.dumps({{"prompt": prompt, "context": context}}).encode("utf-8")
    _req = urllib.request.Request(
        "http://127.0.0.1:{callback_port}/lm_query",
        data=_data,
        headers={{"Content-Type": "application/json"}},
    )
    _resp = urllib.request.urlopen(_req, timeout=120)
    return _json.loads(_resp.read().decode("utf-8"))["result"]

def parallel_lm_query(queries):
    """Query the LLM with multiple prompts in parallel. Each item should be a dict with 'prompt' and 'context' keys. Returns list of response strings."""
    _data = _json.dumps({{"queries": queries}}).encode("utf-8")
    _req = urllib.request.Request(
        "http://127.0.0.1:{callback_port}/lm_query_batch",
        data=_data,
        headers={{"Content-Type": "application/json"}},
    )
    _resp = urllib.request.urlopen(_req, timeout=300)
    return _json.loads(_resp.read().decode("utf-8"))["results"]
"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_contains_variables() {
        let prompt = build_system_prompt(&[("context".to_string(), 50000)], 25);
        assert!(prompt.contains("`context`"));
        assert!(prompt.contains("50,000"));
        assert!(prompt.contains("25 iterations"));
    }

    #[test]
    fn test_system_prompt_no_context() {
        let prompt = build_system_prompt(&[], 10);
        assert!(prompt.contains("No context variables loaded"));
    }

    #[test]
    fn test_lm_query_stubs_contain_port() {
        let stubs = build_lm_query_stubs(12345);
        assert!(stubs.contains("127.0.0.1:12345"));
        assert!(stubs.contains("def lm_query("));
        assert!(stubs.contains("def parallel_lm_query("));
    }
}
