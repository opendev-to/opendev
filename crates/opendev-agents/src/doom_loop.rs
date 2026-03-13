//! Doom-loop cycle detection for the ReAct loop.
//!
//! Mirrors the Python `ReactExecutor._detect_doom_loop()` method from
//! `opendev/repl/react_executor/executor.py`.
//!
//! Tracks recent tool call fingerprints (name + args hash) in a bounded
//! deque and detects repeating cycles of length 1..MAX_CYCLE_LEN.
//! Escalates: 1st = redirect guidance, 2nd = user notification, 3rd = force-stop.

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::Value;

/// Maximum cycle length to check (1-step, 2-step, 3-step).
const MAX_CYCLE_LEN: usize = 3;

/// How many repetitions of a cycle constitute a doom loop.
const DOOM_LOOP_THRESHOLD: usize = 3;

/// Maximum number of recent fingerprints to retain.
const MAX_RECENT: usize = 20;

/// Escalation level returned by the detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoomLoopAction {
    /// No doom loop detected.
    None,
    /// First detection — inject redirect guidance into messages.
    Redirect,
    /// Second detection — notify the user.
    Notify,
    /// Third detection — force-stop the loop.
    ForceStop,
}

/// Recovery strategy recommended when a doom loop is detected.
///
/// The detector returns progressively stronger actions:
/// 1st detection -> `Nudge` (gentle redirect)
/// 2nd detection -> `StepBack` (reconsider approach)
/// 3rd detection -> `CompactContext` (summarize and restart)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Send a nudge message to the model.
    Nudge(String),
    /// Suggest switching to a different approach.
    StepBack(String),
    /// Suggest compacting context to break out of confusion.
    CompactContext,
}

/// Doom-loop cycle detector.
///
/// Maintains a sliding window of tool call fingerprints and checks for
/// repeating patterns after each batch of tool calls.
#[derive(Debug)]
pub struct DoomLoopDetector {
    recent: VecDeque<String>,
    nudge_count: usize,
}

impl DoomLoopDetector {
    /// Create a new detector.
    pub fn new() -> Self {
        Self {
            recent: VecDeque::with_capacity(MAX_RECENT),
            nudge_count: 0,
        }
    }

    /// Current nudge count (number of times a doom loop has been detected).
    pub fn nudge_count(&self) -> usize {
        self.nudge_count
    }

    /// Reset the detector state (e.g., after a force-stop clears the context).
    pub fn reset(&mut self) {
        self.recent.clear();
        self.nudge_count = 0;
    }

    /// Map a detected doom-loop action to a concrete recovery strategy.
    ///
    /// Escalation sequence:
    /// - `Redirect` (1st detection) -> `Nudge` (gentle redirect)
    /// - `Notify` (2nd detection) -> `StepBack` (reconsider approach)
    /// - `ForceStop` (3rd detection) -> `CompactContext`
    /// - `None` -> empty `Nudge` (no-op)
    pub fn recovery_action(&self, action: &DoomLoopAction, warning: &str) -> RecoveryAction {
        match action {
            DoomLoopAction::Redirect => RecoveryAction::Nudge(format!(
                "{warning} Consider trying a different approach or tool."
            )),
            DoomLoopAction::Notify => RecoveryAction::StepBack(format!(
                "Warning: {warning} Please reconsider your approach entirely. \
                 Step back and think about what is fundamentally going wrong, \
                 then try a different strategy."
            )),
            DoomLoopAction::ForceStop => RecoveryAction::CompactContext,
            DoomLoopAction::None => RecoveryAction::Nudge(String::new()),
        }
    }

    /// Compute a compact fingerprint for a tool call: `"tool_name:args_hash"`.
    fn fingerprint(tool_name: &str, args_str: &str) -> String {
        let mut hasher = DefaultHasher::new();
        args_str.hash(&mut hasher);
        let h = hasher.finish();
        format!("{tool_name}:{h:016x}")
    }

    /// Record tool calls and check for a doom loop.
    ///
    /// Returns `(action, warning_message)`. The action tells the caller what
    /// escalation level to apply; the warning is a human-readable description
    /// (empty string when action is `None`).
    pub fn check(&mut self, tool_calls: &[Value]) -> (DoomLoopAction, String) {
        // Append fingerprints for this batch
        for tc in tool_calls {
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");
            let args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let fp = Self::fingerprint(name, args);

            if self.recent.len() >= MAX_RECENT {
                self.recent.pop_front();
            }
            self.recent.push_back(fp);
        }

        // Check for repeating cycles of length 1..MAX_CYCLE_LEN
        let tail: Vec<&String> = self.recent.iter().collect();

        for cycle_len in 1..=MAX_CYCLE_LEN {
            let required = cycle_len * DOOM_LOOP_THRESHOLD;
            if tail.len() < required {
                continue;
            }

            let segment = &tail[tail.len() - required..];
            let pattern = &segment[..cycle_len];
            let is_cycle = segment
                .iter()
                .enumerate()
                .all(|(i, fp)| *fp == pattern[i % cycle_len]);

            if is_cycle {
                self.nudge_count += 1;

                let warning = if cycle_len == 1 {
                    let tool_name = pattern[0].split(':').next().unwrap_or("unknown");
                    format!(
                        "The agent has called `{tool_name}` with the same arguments \
                         {DOOM_LOOP_THRESHOLD} times consecutively. It may be stuck in a loop."
                    )
                } else {
                    let tool_names: Vec<&str> = pattern
                        .iter()
                        .map(|p| p.split(':').next().unwrap_or("unknown"))
                        .collect();
                    format!(
                        "The agent is repeating a {cycle_len}-step cycle \
                         ({}) {DOOM_LOOP_THRESHOLD} times. It may be stuck in a loop.",
                        tool_names.join(" -> ")
                    )
                };

                let action = match self.nudge_count {
                    1 => DoomLoopAction::Redirect,
                    2 => DoomLoopAction::Notify,
                    _ => {
                        self.recent.clear();
                        DoomLoopAction::ForceStop
                    }
                };

                return (action, warning);
            }
        }

        (DoomLoopAction::None, String::new())
    }
}

impl Default for DoomLoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_call(name: &str, args: &str) -> Value {
        serde_json::json!({
            "id": "tc-1",
            "function": {"name": name, "arguments": args}
        })
    }

    #[test]
    fn test_no_doom_loop_varied_calls() {
        let mut det = DoomLoopDetector::new();
        for i in 0..10 {
            let tc = make_tool_call("read_file", &format!("{{\"path\": \"file{i}.rs\"}}"));
            let (action, _) = det.check(&[tc]);
            assert_eq!(action, DoomLoopAction::None);
        }
    }

    #[test]
    fn test_single_step_doom_loop() {
        let mut det = DoomLoopDetector::new();
        let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");

        // First two calls: no doom loop
        let (action, _) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::None);
        let (action, _) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::None);

        // Third identical call: doom loop detected (Redirect)
        let (action, warning) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::Redirect);
        assert!(warning.contains("read_file"));
        assert!(warning.contains("3 times"));

        // Fourth identical call: Notify
        let (action, _) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::Notify);

        // Fifth: ForceStop
        let (action, _) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::ForceStop);
        assert_eq!(det.nudge_count(), 3);
    }

    #[test]
    fn test_two_step_cycle() {
        let mut det = DoomLoopDetector::new();
        let edit = make_tool_call(
            "edit_file",
            "{\"path\": \"a.rs\", \"old\": \"x\", \"new\": \"y\"}",
        );
        let test = make_tool_call("bash", "{\"command\": \"cargo test\"}");

        // Need 2*3=6 calls to detect a 2-step cycle with threshold 3
        for _ in 0..2 {
            let (action, _) = det.check(&[edit.clone()]);
            assert_eq!(action, DoomLoopAction::None);
            let (action, _) = det.check(&[test.clone()]);
            assert_eq!(action, DoomLoopAction::None);
        }
        // 5th call (3rd edit)
        let (action, _) = det.check(&[edit.clone()]);
        assert_eq!(action, DoomLoopAction::None);
        // 6th call (3rd test) — completes 3 repetitions of the 2-step cycle
        let (action, warning) = det.check(&[test.clone()]);
        assert_eq!(action, DoomLoopAction::Redirect);
        assert!(warning.contains("2-step cycle"));
    }

    #[test]
    fn test_reset() {
        let mut det = DoomLoopDetector::new();
        let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");
        det.check(&[tc.clone()]);
        det.check(&[tc.clone()]);
        det.check(&[tc.clone()]);
        assert_eq!(det.nudge_count(), 1);

        det.reset();
        assert_eq!(det.nudge_count(), 0);

        // After reset, no doom loop since history is cleared
        let (action, _) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::None);
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let fp1 = DoomLoopDetector::fingerprint("read_file", "{\"path\": \"a.rs\"}");
        let fp2 = DoomLoopDetector::fingerprint("read_file", "{\"path\": \"a.rs\"}");
        assert_eq!(fp1, fp2);

        let fp3 = DoomLoopDetector::fingerprint("read_file", "{\"path\": \"b.rs\"}");
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn test_batch_tool_calls() {
        let mut det = DoomLoopDetector::new();
        let tc = make_tool_call("search", "{\"query\": \"foo\"}");

        // Submit 3 identical calls in one batch
        let (action, _) = det.check(&[tc.clone(), tc.clone(), tc.clone()]);
        assert_eq!(action, DoomLoopAction::Redirect);
    }

    #[test]
    fn test_three_step_cycle() {
        let mut det = DoomLoopDetector::new();
        let a = make_tool_call("read_file", "{\"path\": \"a\"}");
        let b = make_tool_call("edit_file", "{\"path\": \"b\"}");
        let c = make_tool_call("bash", "{\"cmd\": \"test\"}");

        // 3*3=9 calls for a 3-step cycle
        for round in 0..3 {
            let (action, _) = det.check(&[a.clone()]);
            if round < 2 {
                assert_eq!(action, DoomLoopAction::None);
            }
            let (action, _) = det.check(&[b.clone()]);
            if round < 2 {
                assert_eq!(action, DoomLoopAction::None);
            }
            let (action, warning) = det.check(&[c.clone()]);
            if round < 2 {
                assert_eq!(action, DoomLoopAction::None);
            } else {
                assert_eq!(action, DoomLoopAction::Redirect);
                assert!(warning.contains("3-step cycle"));
            }
        }
    }

    #[test]
    fn test_recovery_action_redirect_returns_nudge() {
        let mut det = DoomLoopDetector::new();
        let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");

        // Trigger first detection (Redirect)
        det.check(&[tc.clone()]);
        det.check(&[tc.clone()]);
        let (action, warning) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::Redirect);

        let recovery = det.recovery_action(&action, &warning);
        match recovery {
            RecoveryAction::Nudge(msg) => {
                assert!(msg.contains("Consider trying a different approach"));
            }
            other => panic!("Expected Nudge, got {:?}", other),
        }
    }

    #[test]
    fn test_recovery_action_notify_returns_step_back() {
        let mut det = DoomLoopDetector::new();
        let tc = make_tool_call("read_file", "{\"path\": \"same.rs\"}");

        // First detection (Redirect)
        det.check(&[tc.clone()]);
        det.check(&[tc.clone()]);
        det.check(&[tc.clone()]);

        // Second detection (Notify)
        let (action, warning) = det.check(&[tc.clone()]);
        assert_eq!(action, DoomLoopAction::Notify);

        let recovery = det.recovery_action(&action, &warning);
        match recovery {
            RecoveryAction::StepBack(msg) => {
                assert!(msg.contains("reconsider your approach"));
            }
            other => panic!("Expected StepBack, got {:?}", other),
        }
    }

    #[test]
    fn test_recovery_action_force_stop_returns_compact() {
        let det = DoomLoopDetector::new();
        let recovery = det.recovery_action(&DoomLoopAction::ForceStop, "stuck");
        assert_eq!(recovery, RecoveryAction::CompactContext);
    }

    #[test]
    fn test_recovery_action_none_returns_empty_nudge() {
        let det = DoomLoopDetector::new();
        let recovery = det.recovery_action(&DoomLoopAction::None, "");
        match recovery {
            RecoveryAction::Nudge(msg) => assert!(msg.is_empty()),
            other => panic!("Expected empty Nudge, got {:?}", other),
        }
    }
}
