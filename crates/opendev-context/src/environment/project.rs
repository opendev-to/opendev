//! Project detection utilities: git helpers, config files, tech stack, directory tree.

use std::path::Path;
use std::process::Command;

/// Run a git command and return trimmed stdout, or None on failure.
pub(super) fn git_cmd(working_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(working_dir)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Detect the default branch (main or master).
pub(super) fn detect_default_branch(working_dir: &Path) -> Option<String> {
    // Try symbolic-ref first
    if let Some(branch) = git_cmd(working_dir, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        return branch
            .strip_prefix("refs/remotes/origin/")
            .map(String::from);
    }
    // Fallback: check if main or master exists
    for branch in &["main", "master"] {
        if git_cmd(
            working_dir,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
        )
        .is_some()
        {
            return Some(branch.to_string());
        }
    }
    None
}

/// Known project configuration files and their tech stacks.
const CONFIG_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", "Rust"),
    ("package.json", "Node.js/JavaScript"),
    ("tsconfig.json", "TypeScript"),
    ("pyproject.toml", "Python"),
    ("setup.py", "Python"),
    ("requirements.txt", "Python"),
    ("go.mod", "Go"),
    ("Gemfile", "Ruby"),
    ("pom.xml", "Java/Maven"),
    ("build.gradle", "Java/Gradle"),
    ("build.gradle.kts", "Kotlin/Gradle"),
    ("Makefile", "Make"),
    ("CMakeLists.txt", "C/C++/CMake"),
    ("docker-compose.yml", "Docker"),
    ("docker-compose.yaml", "Docker"),
    ("Dockerfile", "Docker"),
    (".github/workflows", "GitHub Actions"),
    ("terraform", "Terraform"),
    ("serverless.yml", "Serverless"),
    ("next.config.js", "Next.js"),
    ("next.config.mjs", "Next.js"),
    ("vite.config.ts", "Vite"),
    ("webpack.config.js", "Webpack"),
    ("tailwind.config.js", "Tailwind CSS"),
    ("mix.exs", "Elixir"),
    ("pubspec.yaml", "Flutter/Dart"),
    ("Podfile", "iOS/CocoaPods"),
    (".swift", "Swift"),
];

/// Detect which config files exist in the working directory.
pub(super) fn detect_config_files(working_dir: &Path) -> Vec<String> {
    CONFIG_FILES
        .iter()
        .filter(|(file, _)| working_dir.join(file).exists())
        .map(|(file, _)| file.to_string())
        .collect()
}

/// Infer tech stack from detected config files.
pub(super) fn infer_tech_stack(config_files: &[String]) -> Vec<String> {
    let mut stack: Vec<String> = config_files
        .iter()
        .filter_map(|file| {
            CONFIG_FILES
                .iter()
                .find(|(f, _)| *f == file.as_str())
                .map(|(_, tech)| tech.to_string())
        })
        .collect();
    stack.sort();
    stack.dedup();
    stack
}

/// Build a shallow directory tree (up to given depth).
pub(super) fn build_directory_tree(working_dir: &Path, max_depth: usize) -> Option<String> {
    let mut lines = Vec::new();
    let dir_name = working_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    lines.push(format!("{dir_name}/"));
    collect_tree_entries(working_dir, "", max_depth, 0, &mut lines);
    if lines.len() <= 1 {
        return None;
    }
    Some(lines.join("\n"))
}

/// Recursively collect directory entries for the tree display.
fn collect_tree_entries(
    dir: &Path,
    prefix: &str,
    max_depth: usize,
    current_depth: usize,
    lines: &mut Vec<String>,
) {
    if current_depth >= max_depth {
        return;
    }

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !name.starts_with('.')
                    && name != "node_modules"
                    && name != "target"
                    && name != "dist"
                    && name != "build"
                    && name != "__pycache__"
                    && name != ".git"
                    && name != "vendor"
                    && name != "venv"
                    && name != ".venv"
            })
            .collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    // Limit to 30 entries per directory
    let total = entries.len();
    let show = entries.iter().take(30);

    for (i, entry) in show.enumerate() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_last = i == total.min(30) - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };

        if entry.path().is_dir() {
            lines.push(format!("{prefix}{connector}{name}/"));
            collect_tree_entries(
                &entry.path(),
                &child_prefix,
                max_depth,
                current_depth + 1,
                lines,
            );
        } else {
            lines.push(format!("{prefix}{connector}{name}"));
        }
    }

    if total > 30 {
        lines.push(format!("{prefix}    ... and {} more", total - 30));
    }
}
