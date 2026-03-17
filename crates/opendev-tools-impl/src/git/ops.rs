//! Git operation functions — status, diff, log, branch, checkout, commit, push, pull, stash, merge, create_pr.

use std::collections::HashMap;
use std::process::Command;

use opendev_tools_core::ToolResult;

/// Branches that should never be force-pushed to.
pub(super) const PROTECTED_BRANCHES: &[&str] =
    &["main", "master", "develop", "production", "staging"];

pub(super) fn run_git(args: &[&str], cwd: &str) -> (bool, String) {
    match Command::new("git").args(args).current_dir(cwd).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if output.status.success() {
                (true, stdout.trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                (false, stderr.trim().to_string())
            }
        }
        Err(e) => (false, format!("Failed to execute git: {e}")),
    }
}

pub(super) fn git_status(cwd: &str) -> ToolResult {
    let (ok, out) = run_git(&["status", "--porcelain=v1", "-b"], cwd);
    if !ok {
        return ToolResult::fail(out);
    }

    let lines: Vec<&str> = out.lines().collect();
    let branch_line = lines.first().copied().unwrap_or("");
    let branch = branch_line
        .strip_prefix("## ")
        .unwrap_or("unknown")
        .split("...")
        .next()
        .unwrap_or("unknown");

    // Parse ahead/behind info from branch line
    let mut ahead = 0u64;
    let mut behind = 0u64;
    if let Some(cap) = branch_line.find("ahead ") {
        let rest = &branch_line[cap + 6..];
        if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
            ahead = rest[..end].parse().unwrap_or(0);
        } else {
            // Number goes to the end (minus possible ']')
            ahead = rest.trim_end_matches(']').parse().unwrap_or(0);
        }
    }
    if let Some(cap) = branch_line.find("behind ") {
        let rest = &branch_line[cap + 7..];
        if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
            behind = rest[..end].parse().unwrap_or(0);
        } else {
            behind = rest.trim_end_matches(']').parse().unwrap_or(0);
        }
    }

    let changes: Vec<&str> = lines.iter().skip(1).copied().collect();

    let mut output = format!("Branch: {branch}\n");
    if ahead > 0 {
        output.push_str(&format!("Ahead: {ahead} commits\n"));
    }
    if behind > 0 {
        output.push_str(&format!("Behind: {behind} commits\n"));
    }
    if changes.is_empty() {
        output.push_str("Working tree clean");
    } else {
        output.push_str(&format!("Changes ({}):\n", changes.len()));
        for (i, change) in changes.iter().enumerate() {
            if i >= 50 {
                output.push_str(&format!("  ... and {} more\n", changes.len() - 50));
                break;
            }
            output.push_str(&format!("  {change}\n"));
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert("branch".into(), serde_json::json!(branch));
    metadata.insert("change_count".into(), serde_json::json!(changes.len()));
    metadata.insert("ahead".into(), serde_json::json!(ahead));
    metadata.insert("behind".into(), serde_json::json!(behind));

    ToolResult::ok_with_metadata(output, metadata)
}

pub(super) fn git_diff(cwd: &str, file: Option<&str>, staged: bool) -> ToolResult {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    if let Some(f) = file {
        args.push("--");
        args.push(f);
    }

    let (ok, out) = run_git(&args, cwd);
    if !ok {
        return ToolResult::fail(out);
    }
    ToolResult::ok(if out.is_empty() {
        "No differences found".to_string()
    } else {
        out
    })
}

pub(super) fn git_log(cwd: &str, limit: usize) -> ToolResult {
    let limit_str = format!("-{limit}");
    let (ok, out) = run_git(&["log", &limit_str, "--format=%h %s (%cr) <%an>"], cwd);
    if !ok {
        return ToolResult::fail(out);
    }
    ToolResult::ok(if out.is_empty() {
        "No commits found".to_string()
    } else {
        out
    })
}

pub(super) fn git_branch(cwd: &str, name: Option<&str>) -> ToolResult {
    if let Some(name) = name {
        let (ok, out) = run_git(&["branch", name], cwd);
        if ok {
            ToolResult::ok(format!("Created branch: {name}"))
        } else {
            ToolResult::fail(out)
        }
    } else {
        let (ok, out) = run_git(&["branch", "-a"], cwd);
        if !ok {
            return ToolResult::fail(out);
        }
        ToolResult::ok(out)
    }
}

pub(super) fn git_checkout(cwd: &str, branch: &str, create: bool) -> ToolResult {
    // Safety: check for uncommitted changes
    let (ok, status_out) = run_git(&["status", "--porcelain"], cwd);
    if ok && !status_out.is_empty() {
        let dirty = status_out.lines().count();
        return ToolResult::fail(format!(
            "Working tree has {dirty} uncommitted changes. Commit or stash them first."
        ));
    }

    let mut args = vec!["checkout"];
    if create {
        args.push("-b");
    }
    args.push(branch);

    let (ok, out) = run_git(&args, cwd);
    if ok {
        ToolResult::ok(format!("Switched to branch: {branch}"))
    } else {
        ToolResult::fail(out)
    }
}

pub(super) fn git_commit(cwd: &str, message: &str) -> ToolResult {
    // Check staged changes
    let (ok, staged) = run_git(&["diff", "--cached", "--stat"], cwd);
    if ok && staged.is_empty() {
        return ToolResult::fail("No staged changes to commit. Use 'git add' first.");
    }

    let (ok, out) = run_git(&["commit", "-m", message], cwd);
    if ok {
        ToolResult::ok(out)
    } else {
        ToolResult::fail(out)
    }
}

pub(super) fn git_push(cwd: &str, remote: &str, branch: Option<&str>, force: bool) -> ToolResult {
    if force {
        let target = if let Some(b) = branch {
            b.to_string()
        } else {
            // Resolve current branch via git rev-parse
            let (ok, current) = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], cwd);
            if ok { current } else { String::new() }
        };
        if PROTECTED_BRANCHES.contains(&target.as_str()) {
            return ToolResult::fail(format!(
                "Refusing force-push to protected branch '{target}'. This could destroy shared history."
            ));
        }
    }

    let mut args = vec!["push", remote];
    if let Some(b) = branch {
        args.push(b);
    }
    if force {
        args.push("--force-with-lease");
    }

    let (ok, out) = run_git(&args, cwd);
    if ok {
        ToolResult::ok(if out.is_empty() {
            "Push successful".to_string()
        } else {
            out
        })
    } else {
        ToolResult::fail(out)
    }
}

pub(super) fn git_pull(cwd: &str, remote: &str, branch: Option<&str>) -> ToolResult {
    let mut args = vec!["pull", remote];
    if let Some(b) = branch {
        args.push(b);
    }

    let (ok, out) = run_git(&args, cwd);
    if ok {
        ToolResult::ok(out)
    } else {
        ToolResult::fail(out)
    }
}

pub(super) fn git_stash(cwd: &str, action: &str, message: Option<&str>) -> ToolResult {
    let args: Vec<&str> = match action {
        "push" | "save" => {
            if let Some(msg) = message {
                // Can't use a simple slice because of the borrow; build below
                let (ok, out) = run_git(&["stash", "push", "-m", msg], cwd);
                return if ok {
                    ToolResult::ok(if out.is_empty() {
                        "Stash saved".to_string()
                    } else {
                        out
                    })
                } else {
                    ToolResult::fail(out)
                };
            }
            vec!["stash", "push"]
        }
        "pop" => vec!["stash", "pop"],
        "list" => vec!["stash", "list"],
        "drop" => vec!["stash", "drop"],
        "show" => vec!["stash", "show", "-p"],
        _ => {
            return ToolResult::fail(format!(
                "Unknown stash action: {action}. Available: push, pop, list, drop, show"
            ));
        }
    };

    let (ok, out) = run_git(&args, cwd);
    if !ok {
        return ToolResult::fail(out);
    }
    ToolResult::ok(if out.is_empty() {
        match action {
            "list" => "No stashes".to_string(),
            "push" | "save" => "Stash saved".to_string(),
            "pop" => "Stash applied and dropped".to_string(),
            "drop" => "Stash dropped".to_string(),
            _ => out,
        }
    } else {
        out
    })
}

pub(super) fn git_merge(cwd: &str, branch: &str) -> ToolResult {
    let (ok, out) = run_git(&["merge", branch], cwd);
    if ok {
        ToolResult::ok(out)
    } else {
        ToolResult::fail(out)
    }
}

pub(super) fn git_create_pr(cwd: &str, title: &str, body: &str, base: Option<&str>) -> ToolResult {
    let mut args = vec!["pr", "create", "--title", title, "--body", body];
    if let Some(b) = base {
        args.push("--base");
        args.push(b);
    }

    match Command::new("gh").args(&args).current_dir(cwd).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if output.status.success() {
                ToolResult::ok(stdout)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let error = if stderr.is_empty() { stdout } else { stderr };
                if error.to_lowercase().contains("not found")
                    || error.to_lowercase().contains("command not found")
                {
                    ToolResult::fail(
                        "GitHub CLI (gh) is not installed. Install it with: brew install gh"
                            .to_string(),
                    )
                } else {
                    ToolResult::fail(error)
                }
            }
        }
        Err(_) => ToolResult::fail(
            "GitHub CLI (gh) is not installed. Install it with: brew install gh".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_git_nonexistent() {
        let (ok, _) = run_git(&["status"], "/nonexistent/path");
        assert!(!ok);
    }

    #[test]
    fn test_git_force_push_protected() {
        let result = git_push("/tmp", "origin", Some("main"), true);
        assert!(!result.success);
        assert!(result.error.unwrap().contains("protected branch"));
    }

    #[test]
    fn test_git_stash_unknown_action() {
        let result = git_stash("/tmp", "invalid_action", None);
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown stash action"));
    }

    #[test]
    fn test_git_status_parses_ahead_behind() {
        // Simulate parsing ahead/behind from branch line
        let branch_line = "## main...origin/main [ahead 3, behind 2]";
        let mut ahead = 0u64;
        let mut behind = 0u64;
        if let Some(cap) = branch_line.find("ahead ") {
            let rest = &branch_line[cap + 6..];
            if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                ahead = rest[..end].parse().unwrap_or(0);
            }
        }
        if let Some(cap) = branch_line.find("behind ") {
            let rest = &branch_line[cap + 7..];
            if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                behind = rest[..end].parse().unwrap_or(0);
            }
        }
        assert_eq!(ahead, 3);
        assert_eq!(behind, 2);
    }

    #[test]
    fn test_git_status_no_ahead_behind() {
        let branch_line = "## main...origin/main";
        let mut ahead = 0u64;
        let mut behind = 0u64;
        if let Some(cap) = branch_line.find("ahead ") {
            let rest = &branch_line[cap + 6..];
            if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                ahead = rest[..end].parse().unwrap_or(0);
            }
        }
        if let Some(cap) = branch_line.find("behind ") {
            let rest = &branch_line[cap + 7..];
            if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                behind = rest[..end].parse().unwrap_or(0);
            }
        }
        assert_eq!(ahead, 0);
        assert_eq!(behind, 0);
    }

    #[test]
    fn test_git_force_push_resolves_branch() {
        let result = git_push("/tmp", "origin", None, true);
        assert!(!result.success);
    }

    #[test]
    fn test_protected_branches_list() {
        assert!(PROTECTED_BRANCHES.contains(&"main"));
        assert!(PROTECTED_BRANCHES.contains(&"master"));
        assert!(PROTECTED_BRANCHES.contains(&"develop"));
        assert!(PROTECTED_BRANCHES.contains(&"production"));
        assert!(PROTECTED_BRANCHES.contains(&"staging"));
        assert!(!PROTECTED_BRANCHES.contains(&"feature/my-branch"));
    }
}
