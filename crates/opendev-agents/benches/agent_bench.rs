//! Criterion benchmarks for agent hot paths.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::collections::HashMap;

use opendev_agents::doom_loop::DoomLoopDetector;
use opendev_agents::prompts::composer::create_default_composer;

// ---------------------------------------------------------------------------
// DoomLoopDetector benchmarks
// ---------------------------------------------------------------------------

fn make_tool_call(name: &str, args: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "tc-1",
        "function": {"name": name, "arguments": args}
    })
}

fn bench_doom_loop_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("DoomLoopDetector::check");

    // Varied calls (no doom loop detected)
    group.bench_function("varied_10_calls", |b| {
        b.iter(|| {
            let mut det = DoomLoopDetector::new();
            for i in 0..10 {
                let tc = make_tool_call("read_file", &format!("{{\"path\": \"file{i}.rs\"}}"));
                det.check(black_box(&[tc]));
            }
        });
    });

    // Identical calls (doom loop triggers)
    group.bench_function("identical_10_calls", |b| {
        b.iter(|| {
            let mut det = DoomLoopDetector::new();
            let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");
            for _ in 0..10 {
                det.check(black_box(&[tc.clone()]));
            }
        });
    });

    // Large batch of varied calls
    group.bench_function("varied_100_calls", |b| {
        b.iter(|| {
            let mut det = DoomLoopDetector::new();
            for i in 0..100 {
                let tc = make_tool_call(
                    if i % 3 == 0 {
                        "read_file"
                    } else if i % 3 == 1 {
                        "edit_file"
                    } else {
                        "bash"
                    },
                    &format!("{{\"arg\": \"{i}\"}}"),
                );
                det.check(black_box(&[tc]));
            }
        });
    });

    // Mixed cycle pattern (2-step cycles)
    group.bench_function("two_step_cycle_20_calls", |b| {
        let edit = make_tool_call("edit_file", "{\"path\": \"a.rs\"}");
        let test = make_tool_call("bash", "{\"command\": \"cargo test\"}");
        b.iter(|| {
            let mut det = DoomLoopDetector::new();
            for _ in 0..10 {
                det.check(black_box(&[edit.clone()]));
                det.check(black_box(&[test.clone()]));
            }
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// PromptComposer benchmarks
// ---------------------------------------------------------------------------

fn bench_prompt_compose(c: &mut Criterion) {
    let mut group = c.benchmark_group("PromptComposer::compose");

    // Compose with empty context (only unconditional sections)
    group.bench_function("default_no_conditions", |b| {
        let dir = tempfile::TempDir::new().unwrap();
        let composer = create_default_composer(dir.path());
        let ctx = HashMap::new();
        b.iter(|| {
            composer.compose(black_box(&ctx));
        });
    });

    // Compose with full context (all conditions met)
    group.bench_function("default_all_conditions", |b| {
        let dir = tempfile::TempDir::new().unwrap();
        let composer = create_default_composer(dir.path());
        let mut ctx = HashMap::new();
        ctx.insert("has_subagents".to_string(), serde_json::json!(true));
        ctx.insert("in_git_repo".to_string(), serde_json::json!(true));
        ctx.insert("todo_tracking_enabled".to_string(), serde_json::json!(true));
        ctx.insert("model_provider".to_string(), serde_json::json!("anthropic"));
        ctx.insert("session_id".to_string(), serde_json::json!("test-123"));
        b.iter(|| {
            composer.compose(black_box(&ctx));
        });
    });

    // Two-part composition (cache-split)
    group.bench_function("two_part_split", |b| {
        let dir = tempfile::TempDir::new().unwrap();
        let composer = create_default_composer(dir.path());
        let mut ctx = HashMap::new();
        ctx.insert("in_git_repo".to_string(), serde_json::json!(true));
        ctx.insert("session_id".to_string(), serde_json::json!("test-123"));
        b.iter(|| {
            composer.compose_two_part(black_box(&ctx));
        });
    });

    // Compose with variable substitution
    group.bench_function("compose_with_vars", |b| {
        let dir = tempfile::TempDir::new().unwrap();
        let composer = create_default_composer(dir.path());
        let ctx = HashMap::new();
        let mut vars = HashMap::new();
        vars.insert("session_id".to_string(), "abc-123".to_string());
        vars.insert("working_dir".to_string(), "/home/user/project".to_string());
        vars.insert("model".to_string(), "claude-sonnet-4".to_string());
        b.iter(|| {
            composer.compose_with_vars(black_box(&ctx), black_box(&vars));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_doom_loop_check, bench_prompt_compose);
criterion_main!(benches);
