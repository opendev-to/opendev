use super::*;

#[test]
fn test_is_safe_command() {
    assert!(is_safe_command("ls"));
    assert!(is_safe_command("ls -la"));
    assert!(is_safe_command("git status"));
    assert!(is_safe_command("git diff --staged"));
    assert!(is_safe_command("cat foo.txt"));
    assert!(!is_safe_command("rm -rf /"));
    assert!(!is_safe_command("catastrophe")); // must not match "cat"
    assert!(!is_safe_command(""));
}

#[test]
fn test_safe_command_case_insensitive() {
    assert!(is_safe_command("LS -la"));
    assert!(is_safe_command("Git Status"));
}

#[test]
fn test_safe_command_build_tools() {
    assert!(is_safe_command("cargo test --workspace"));
    assert!(is_safe_command("cargo build --release -p opendev-cli"));
    assert!(is_safe_command("cargo clippy --workspace -- -D warnings"));
    assert!(is_safe_command("cargo fmt"));
    assert!(is_safe_command("npm run build"));
    assert!(is_safe_command("npm test"));
    assert!(is_safe_command("npm ci"));
    assert!(is_safe_command("make"));
    assert!(is_safe_command("go test ./..."));
    assert!(is_safe_command("pip install -r requirements.txt"));
    assert!(is_safe_command("bazel build //..."));
    assert!(is_safe_command("mvn test -pl core"));
}

#[test]
fn test_safe_command_version_checks() {
    assert!(is_safe_command("ruby --version"));
    assert!(is_safe_command("java --version"));
    assert!(is_safe_command("rustc --version"));
    assert!(is_safe_command("rustup show"));
    assert!(is_safe_command("deno --version"));
}

#[test]
fn test_safe_command_linters_and_testing() {
    assert!(is_safe_command("eslint src/"));
    assert!(is_safe_command("prettier --check ."));
    assert!(is_safe_command("ruff check ."));
    assert!(is_safe_command("pytest -v tests/"));
    assert!(is_safe_command("jest --coverage"));
    assert!(is_safe_command("dotnet test"));
    assert!(is_safe_command("flutter test"));
}

#[test]
fn test_safe_command_git_extended() {
    assert!(is_safe_command("git blame src/main.rs"));
    assert!(is_safe_command("git rev-parse HEAD"));
    assert!(is_safe_command("git ls-files"));
    assert!(is_safe_command("git config --get user.name"));
    assert!(is_safe_command("git describe --tags"));
}

#[test]
fn test_safe_command_containers_and_ci() {
    assert!(is_safe_command("docker ps -a"));
    assert!(is_safe_command("docker images"));
    assert!(is_safe_command("kubectl get pods"));
    assert!(is_safe_command("kubectl describe deployment foo"));
    assert!(is_safe_command("gh pr view 123"));
    assert!(is_safe_command("gh run list"));
    assert!(is_safe_command("terraform plan"));
}

#[test]
fn test_safe_command_system_info() {
    assert!(is_safe_command("uname -a"));
    assert!(is_safe_command("whoami"));
    assert!(is_safe_command("ps aux"));
    assert!(is_safe_command("curl https://example.com"));
    assert!(is_safe_command("dig example.com"));
    assert!(is_safe_command("ping -c 1 localhost"));
}

#[test]
fn test_safe_command_text_processing() {
    assert!(is_safe_command("jq '.foo' data.json"));
    assert!(is_safe_command("sort -u file.txt"));
    assert!(is_safe_command("awk '{print $1}' file.txt"));
    assert!(is_safe_command("sed -n '1,10p' file.txt"));
    assert!(is_safe_command("tar tf archive.tar.gz"));
    assert!(is_safe_command("unzip -l archive.zip"));
}

#[test]
fn test_unsafe_commands_still_blocked() {
    assert!(!is_safe_command("rm -rf /"));
    assert!(!is_safe_command("docker run ubuntu"));
    assert!(!is_safe_command("kubectl delete pod foo"));
    assert!(!is_safe_command("git push"));
    assert!(!is_safe_command("git reset --hard"));
    assert!(!is_safe_command("chmod 777 /etc/passwd"));
    assert!(!is_safe_command("sudo anything"));
}

// ── Shell-aware parsing tests ──

#[test]
fn test_env_var_prefix_stripped() {
    assert!(is_safe_command("RUST_LOG=debug cargo test"));
    assert!(is_safe_command("CI=true FORCE_COLOR=1 npm test"));
    assert!(is_safe_command("NODE_ENV=test jest --coverage"));
    assert!(is_safe_command("GOFLAGS=-count=1 go test ./..."));
}

#[test]
fn test_path_prefix_stripped() {
    assert!(is_safe_command("/usr/bin/git status"));
    assert!(is_safe_command("/usr/local/bin/cargo test"));
    assert!(is_safe_command("./node_modules/.bin/jest"));
    assert!(is_safe_command("./node_modules/.bin/eslint src/"));
}

#[test]
fn test_env_and_path_combined() {
    assert!(is_safe_command(
        "RUST_LOG=info /usr/bin/cargo test --workspace"
    ));
}

#[test]
fn test_chained_safe_commands() {
    assert!(is_safe_command("cargo fmt && cargo clippy"));
    assert!(is_safe_command("cargo check && cargo test && cargo clippy"));
    assert!(is_safe_command("cd src; ls -la"));
}

#[test]
fn test_chained_with_unsafe_blocked() {
    assert!(!is_safe_command("cargo test && rm -rf /"));
    assert!(!is_safe_command("ls; rm file.txt"));
    assert!(!is_safe_command("git status && git push"));
    assert!(!is_safe_command("echo hello || sudo reboot"));
}

#[test]
fn test_pipe_all_segments_safe() {
    assert!(is_safe_command("git log | grep fix"));
    assert!(is_safe_command("ps aux | grep node"));
    assert!(is_safe_command("cat file.txt | sort | uniq"));
}

#[test]
fn test_pipe_with_unsafe_blocked() {
    assert!(!is_safe_command("ls | sudo tee /etc/passwd"));
}

#[test]
fn test_shell_injection_blocked() {
    assert!(!is_safe_command("echo $(rm -rf /)"));
    assert!(!is_safe_command("ls `whoami`"));
    assert!(!is_safe_command("diff <(cat a) <(cat b)"));
}

#[test]
fn test_file_redirect_blocked() {
    assert!(!is_safe_command("echo hello > /etc/passwd"));
    assert!(!is_safe_command("cat foo >> bar.txt"));
}

#[test]
fn test_stderr_redirect_allowed() {
    assert!(is_safe_command("cargo test 2>&1"));
}

#[test]
fn test_is_env_assignment_helper() {
    assert!(is_env_assignment("FOO=bar"));
    assert!(is_env_assignment("_VAR=1"));
    assert!(is_env_assignment("NODE_ENV=production"));
    assert!(!is_env_assignment("git"));
    assert!(!is_env_assignment("=bad"));
    assert!(!is_env_assignment("123=bad"));
}

#[test]
fn test_normalize_segment_helper() {
    assert_eq!(normalize_segment("cargo test"), "cargo test");
    assert_eq!(normalize_segment("RUST_LOG=debug cargo test"), "cargo test");
    assert_eq!(normalize_segment("/usr/bin/git status"), "git status");
    assert_eq!(
        normalize_segment("CI=1 /usr/local/bin/cargo clippy"),
        "cargo clippy"
    );
}

#[test]
fn test_split_shell_segments_helper() {
    assert_eq!(split_shell_segments("ls"), vec!["ls"]);
    assert_eq!(
        split_shell_segments("cargo fmt && cargo test"),
        vec!["cargo fmt", "cargo test"]
    );
    assert_eq!(
        split_shell_segments("a; b || c && d"),
        vec!["a", "b", "c", "d"]
    );
    assert_eq!(split_shell_segments("a | b | c"), vec!["a", "b", "c"]);
}

#[test]
fn test_extract_command_prefix() {
    assert_eq!(
        extract_command_prefix("cargo test --workspace"),
        "cargo test"
    );
    assert_eq!(extract_command_prefix("git status"), "git status");
    assert_eq!(extract_command_prefix("eslint src/"), "eslint");
    assert_eq!(
        extract_command_prefix("RUST_LOG=debug cargo build"),
        "cargo build"
    );
    assert_eq!(
        extract_command_prefix("/usr/bin/cargo clippy"),
        "cargo clippy"
    );
    assert_eq!(extract_command_prefix("npm run build"), "npm run");
    assert_eq!(extract_command_prefix("cargo --version"), "cargo");
}

#[test]
fn test_autonomy_level_display() {
    assert_eq!(AutonomyLevel::Manual.to_string(), "Manual");
    assert_eq!(AutonomyLevel::SemiAuto.to_string(), "Semi-Auto");
    assert_eq!(AutonomyLevel::Auto.to_string(), "Auto");
}

#[test]
fn test_autonomy_level_parse() {
    assert_eq!(
        AutonomyLevel::from_str_loose("manual"),
        Some(AutonomyLevel::Manual)
    );
    assert_eq!(
        AutonomyLevel::from_str_loose("Semi-Auto"),
        Some(AutonomyLevel::SemiAuto)
    );
    assert_eq!(
        AutonomyLevel::from_str_loose("auto"),
        Some(AutonomyLevel::Auto)
    );
    assert_eq!(AutonomyLevel::from_str_loose("garbage"), None);
}

#[test]
fn test_autonomy_level_serde_roundtrip() {
    let level = AutonomyLevel::SemiAuto;
    let json = serde_json::to_string(&level).unwrap();
    assert_eq!(json, "\"Semi-Auto\"");
    let deserialized: AutonomyLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, level);
}
