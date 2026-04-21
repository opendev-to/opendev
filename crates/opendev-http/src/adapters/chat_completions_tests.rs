use super::*;
use crate::streaming::StreamEvent;
use serde_json::json;

/// Baseline: a standard OpenAI-compat chunk with string-encoded args
/// must parse into a `FunctionCallDelta` carrying that string verbatim.
#[test]
fn test_parse_tool_call_args_as_string_delta() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "function": { "arguments": "{\"file_path\":\"a.txt\"}" }
                }]
            }
        }]
    });
    let event = ChatCompletionsAdapter::parse_chat_completions_sse(&chunk);
    match event {
        Some(StreamEvent::FunctionCallDelta { index, delta }) => {
            assert_eq!(index, 0);
            assert_eq!(delta, "{\"file_path\":\"a.txt\"}");
        }
        other => panic!("expected FunctionCallDelta, got {other:?}"),
    }
}

/// Regression (z.ai GLM-5.1, observed 2026-04-19): GLM-5.1 in OpenAI-compat
/// mode emits `function.arguments` as a JSON *object* instead of the
/// spec-mandated JSON-encoded string. The previous adapter silently dropped
/// the delta, so the downstream tool-call was dispatched with empty args and
/// failed param validation on every iteration.
///
/// Expected: the adapter serializes the object and emits it as a
/// `FunctionCallDelta` carrying the JSON string form.
#[test]
fn test_parse_tool_call_args_as_object_is_serialized() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "function": {
                        "arguments": { "file_path": "hello.txt", "content": "world" }
                    }
                }]
            }
        }]
    });
    let event = ChatCompletionsAdapter::parse_chat_completions_sse(&chunk);
    let (index, delta) = match event {
        Some(StreamEvent::FunctionCallDelta { index, delta }) => (index, delta),
        other => panic!("expected FunctionCallDelta, got {other:?}"),
    };
    assert_eq!(index, 0);
    let parsed: serde_json::Value =
        serde_json::from_str(&delta).expect("emitted delta must be valid JSON");
    assert_eq!(parsed["file_path"], "hello.txt");
    assert_eq!(parsed["content"], "world");
}

/// Defensive: a chunk where `arguments` is present but empty (string "")
/// should NOT generate an event — empty deltas would pollute the
/// accumulator with no-op entries.
#[test]
fn test_parse_tool_call_empty_string_args_no_event() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "function": { "arguments": "" }
                }]
            }
        }]
    });
    let event = ChatCompletionsAdapter::parse_chat_completions_sse(&chunk);
    assert!(
        !matches!(event, Some(StreamEvent::FunctionCallDelta { .. })),
        "empty string args must not emit a FunctionCallDelta, got {event:?}"
    );
}

/// First-chunk style (id + name present) still emits `FunctionCallStart`,
/// regardless of whether args are string or object in that same chunk.
#[test]
fn test_parse_tool_call_start_with_object_args() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_abc",
                    "function": {
                        "name": "Write",
                        "arguments": { "file_path": "a.txt" }
                    }
                }]
            }
        }]
    });
    let event = ChatCompletionsAdapter::parse_chat_completions_sse(&chunk);
    match event {
        Some(StreamEvent::FunctionCallStart {
            index,
            call_id,
            name,
            ..
        }) => {
            assert_eq!(index, 0);
            assert_eq!(call_id, "call_abc");
            assert_eq!(name, "Write");
        }
        other => panic!("expected FunctionCallStart, got {other:?}"),
    }
}

/// Regression (z.ai GLM-5.1, observed 2026-04-19): GLM emits the *entire*
/// tool call — id, name, AND the full arguments string — in one SSE chunk.
/// The parser must surface those args (via `initial_args`) so the accumulator
/// downstream doesn't end up with an empty-args tool call and drop the call.
///
/// Real payload captured from z.ai GLM-5.1 chat.completions streaming:
/// ```json
/// {"choices":[{"index":0,"delta":{"tool_calls":[{
///   "id":"call_c3ebf1e3ec8d4a4981472f38","index":0,"type":"function",
///   "function":{"name":"Write",
///     "arguments":"{\"file_path\":\"hello.txt\",\"content\":\"world\"}"}
/// }]}}]}
/// ```
#[test]
fn test_parse_tool_call_id_name_and_full_args_in_one_chunk() {
    let chunk = json!({
        "choices": [{
            "index": 0,
            "delta": {
                "tool_calls": [{
                    "id": "call_c3ebf1e3ec8d4a4981472f38",
                    "index": 0,
                    "type": "function",
                    "function": {
                        "name": "Write",
                        "arguments": "{\"file_path\":\"hello.txt\",\"content\":\"world\"}"
                    }
                }]
            }
        }]
    });
    let event = ChatCompletionsAdapter::parse_chat_completions_sse(&chunk);
    let (call_id, name, initial_args) = match event {
        Some(StreamEvent::FunctionCallStart {
            call_id,
            name,
            initial_args,
            ..
        }) => (call_id, name, initial_args),
        other => panic!("expected FunctionCallStart carrying initial_args, got {other:?}"),
    };
    assert_eq!(call_id, "call_c3ebf1e3ec8d4a4981472f38");
    assert_eq!(name, "Write");
    let args_str = initial_args.expect("initial_args must be populated when args present");
    let parsed: serde_json::Value =
        serde_json::from_str(&args_str).expect("initial_args must be valid JSON");
    assert_eq!(parsed["file_path"], "hello.txt");
    assert_eq!(parsed["content"], "world");
}

/// id + name chunk without args in the same chunk must still work (the
/// legacy OpenAI-Chat-Completions pattern — start event carries no args,
/// args arrive as subsequent `FunctionCallDelta` chunks).
#[test]
fn test_parse_tool_call_start_without_args_leaves_initial_args_none() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_xyz",
                    "function": { "name": "Read" }
                }]
            }
        }]
    });
    let event = ChatCompletionsAdapter::parse_chat_completions_sse(&chunk);
    match event {
        Some(StreamEvent::FunctionCallStart {
            call_id,
            initial_args,
            ..
        }) => {
            assert_eq!(call_id, "call_xyz");
            assert!(
                initial_args.is_none(),
                "no args in chunk → initial_args must be None, got {initial_args:?}"
            );
        }
        other => panic!("expected FunctionCallStart, got {other:?}"),
    }
}
