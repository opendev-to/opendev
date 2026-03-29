//! Process tree management for cleaning up MCP server child processes.

use tracing::debug;

/// Recursively collect all descendant PIDs of a process using `pgrep -P`.
///
/// Uses BFS to traverse the process tree, collecting all child and grandchild
/// PIDs. This is needed to clean up processes spawned by MCP servers (e.g.,
/// Chrome spawned by chrome-devtools-mcp).
#[cfg(unix)]
pub(super) fn collect_descendant_pids(root_pid: u32) -> Vec<u32> {
    let mut descendants = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root_pid);

    while let Some(pid) = queue.pop_front() {
        match std::process::Command::new("pgrep")
            .args(["-P", &pid.to_string()])
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(child_pid) = line.trim().parse::<u32>() {
                        descendants.push(child_pid);
                        queue.push_back(child_pid);
                    }
                }
            }
            _ => {}
        }
    }

    descendants
}

#[cfg(not(unix))]
pub(super) fn collect_descendant_pids(_root_pid: u32) -> Vec<u32> {
    Vec::new()
}

/// Send SIGTERM to each descendant PID, logging any that fail.
#[cfg(unix)]
pub(super) fn kill_descendant_pids(pids: &[u32]) {
    for &pid in pids {
        debug!("Killing descendant MCP process {}", pid);
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }
}

#[cfg(not(unix))]
pub(super) fn kill_descendant_pids(_pids: &[u32]) {}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
