//! Codebase indexer for generating concise project summaries.
//!
//! Scans a working directory to produce a markdown overview including
//! project structure, key files, and dependencies.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::token_monitor::ContextTokenMonitor;

/// Generate concise codebase summaries for context injection.
#[derive(Debug)]
pub struct CodebaseIndexer {
    working_dir: PathBuf,
    token_monitor: ContextTokenMonitor,
    #[allow(dead_code)]
    target_tokens: usize,
}

impl CodebaseIndexer {
    /// Create a new indexer rooted at the given directory.
    pub fn new(working_dir: &Path) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
            token_monitor: ContextTokenMonitor::new(),
            target_tokens: 3000,
        }
    }

    /// Generate a complete index of the codebase, compressed to fit within `max_tokens`.
    pub fn generate_index(&self, max_tokens: usize) -> String {
        let mut sections = Vec::new();

        let dir_name = self
            .working_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        sections.push(format!("# {}\n", dir_name));
        sections.push(self.generate_overview());
        sections.push(self.generate_structure());
        sections.push(self.generate_key_files());

        if let Some(deps) = self.generate_dependencies() {
            sections.push(deps);
        }

        let content = sections.join("\n\n");
        let tokens = self.token_monitor.count_tokens(&content);
        if tokens > max_tokens {
            self.compress_content(&content, max_tokens)
        } else {
            content
        }
    }

    /// Generate the overview section: file count, README excerpt, and project type.
    pub fn generate_overview(&self) -> String {
        let mut lines = vec!["## Overview\n".to_string()];

        // Count files
        if let Ok(count) = self.count_files() {
            lines.push(format!("**Total Files:** {}", count));
        }

        // README excerpt
        if let Some(readme_path) = self.find_readme()
            && let Ok(content) = fs::read_to_string(&readme_path)
            && let Some(desc) = Self::extract_description(&content, 300)
        {
            lines.push(String::new());
            lines.push(desc);
        }

        // Project type
        if let Some(project_type) = self.detect_project_type() {
            lines.push(String::new());
            lines.push(format!("**Type:** {}", project_type));
        }

        lines.join("\n")
    }

    /// Generate the directory structure section using `tree` or a basic fallback.
    pub fn generate_structure(&self) -> String {
        let mut lines = vec!["## Structure\n".to_string(), "```".to_string()];

        let tree_output = Command::new("tree")
            .args([
                "-L",
                "2",
                "-I",
                "node_modules|__pycache__|.git|venv|build|dist|target",
            ])
            .current_dir(&self.working_dir)
            .output();

        match tree_output {
            Ok(output) if output.status.success() => {
                let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if text.len() > 1500 {
                    let truncated: Vec<&str> = text.lines().take(30).collect();
                    text = truncated.join("\n") + "\n... (truncated)";
                }
                lines.push(text);
            }
            _ => {
                lines.push(self.basic_structure());
            }
        }

        lines.push("```".to_string());
        lines.join("\n")
    }

    /// Generate a listing of key files by category (Main, Config, Tests, Docs).
    pub fn generate_key_files(&self) -> String {
        let mut lines = vec!["## Key Files\n".to_string()];

        let categories: Vec<(&str, Vec<&str>)> = vec![
            (
                "Main",
                vec![
                    "main.py",
                    "index.js",
                    "app.py",
                    "server.py",
                    "main.rs",
                    "lib.rs",
                ],
            ),
            (
                "Config",
                vec![
                    "setup.py",
                    "package.json",
                    "pyproject.toml",
                    "requirements.txt",
                    "Cargo.toml",
                    "Dockerfile",
                ],
            ),
            ("Tests", vec!["test_*.py", "*_test.py", "*_test.rs"]),
            ("Docs", vec!["README.md", "CHANGELOG.md"]),
        ];

        for (category, patterns) in &categories {
            let found = self.find_files(patterns);
            if !found.is_empty() {
                lines.push(format!("\n### {}", category));
                for f in found.iter().take(5) {
                    if let Ok(rel) = f.strip_prefix(&self.working_dir) {
                        lines.push(format!("- `{}`", rel.display()));
                    }
                }
            }
        }

        lines.join("\n")
    }

    /// Parse dependency files (requirements.txt, package.json, Cargo.toml).
    ///
    /// Returns `None` if no dependency files are found.
    pub fn generate_dependencies(&self) -> Option<String> {
        let mut deps: Vec<(&str, Vec<String>)> = Vec::new();

        // requirements.txt
        let req_file = self.working_dir.join("requirements.txt");
        if let Ok(content) = fs::read_to_string(&req_file) {
            let py_deps: Vec<String> = content
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    !trimmed.is_empty() && !trimmed.starts_with('#')
                })
                .take(10)
                .map(|line| line.trim().to_string())
                .collect();
            if !py_deps.is_empty() {
                deps.push(("Python", py_deps));
            }
        }

        // package.json
        let package_json = self.working_dir.join("package.json");
        if let Ok(content) = fs::read_to_string(&package_json)
            && let Ok(data) = serde_json::from_str::<serde_json::Value>(&content)
            && let Some(obj) = data.get("dependencies").and_then(|d| d.as_object())
        {
            let node_deps: Vec<String> = obj.keys().take(10).map(|k| k.to_string()).collect();
            if !node_deps.is_empty() {
                deps.push(("Node", node_deps));
            }
        }

        // Cargo.toml
        let cargo_toml = self.working_dir.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = fs::read_to_string(&cargo_toml)
        {
            let rust_deps: Vec<String> = content
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    trimmed.contains(" = ")
                        && !trimmed.starts_with('[')
                        && !trimmed.starts_with('#')
                })
                .take(10)
                .map(|line| line.split('=').next().unwrap_or("").trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !rust_deps.is_empty() {
                deps.push(("Rust", rust_deps));
            }
        }

        if deps.is_empty() {
            return None;
        }

        let mut lines = vec!["## Dependencies\n".to_string()];
        for (tech, dep_list) in &deps {
            lines.push(format!("\n### {}", tech));
            for dep in dep_list {
                lines.push(format!("- {}", dep));
            }
            if dep_list.len() >= 10 {
                lines.push("- *(and more...)*".to_string());
            }
        }

        Some(lines.join("\n"))
    }

    /// Detect the project type based on indicator files.
    pub fn detect_project_type(&self) -> Option<&str> {
        let indicators: &[(&str, &[&str])] = &[
            (
                "Python",
                &["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"],
            ),
            ("Node", &["package.json", "yarn.lock", "pnpm-lock.yaml"]),
            ("Rust", &["Cargo.toml"]),
            ("Go", &["go.mod"]),
            ("Java", &["pom.xml", "build.gradle"]),
        ];

        for (project_type, files) in indicators {
            if files.iter().any(|f| self.working_dir.join(f).exists()) {
                return Some(project_type);
            }
        }
        None
    }

    /// Compress content to fit within a token budget by trimming paragraphs.
    pub fn compress_content(&self, content: &str, max_tokens: usize) -> String {
        let paragraphs: Vec<&str> = content.split("\n\n").collect();
        let mut compressed: Vec<&str> = Vec::new();

        for paragraph in &paragraphs {
            compressed.push(paragraph);
            let joined = compressed.join("\n\n");
            let tokens = self.token_monitor.count_tokens(&joined);
            if tokens >= max_tokens {
                break;
            }
        }

        compressed.join("\n\n")
    }

    // -- Private helpers --

    fn count_files(&self) -> Result<usize, std::io::Error> {
        fn count_recursive(dir: &Path) -> Result<usize, std::io::Error> {
            let mut count = 0;
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Skip hidden and common build directories
                if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                    continue;
                }
                if path.is_file() {
                    count += 1;
                } else if path.is_dir() {
                    count += count_recursive(&path)?;
                }
            }
            Ok(count)
        }
        count_recursive(&self.working_dir)
    }

    fn find_readme(&self) -> Option<PathBuf> {
        for pattern in &["README.md", "README.rst", "README.txt", "README"] {
            let readme = self.working_dir.join(pattern);
            if readme.exists() {
                return Some(readme);
            }
        }
        None
    }

    fn extract_description(content: &str, max_length: usize) -> Option<String> {
        let paragraphs: Vec<&str> = content
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();

        let description = paragraphs.first()?;
        if description.len() > max_length {
            Some(format!("{}...", &description[..max_length - 3]))
        } else {
            Some(description.to_string())
        }
    }

    fn find_files(&self, patterns: &[&str]) -> Vec<PathBuf> {
        let mut matches = Vec::new();
        for pattern in patterns {
            if let Some(name) = pattern.strip_prefix("*_") {
                // Suffix pattern like *_test.py
                self.walk_files(&self.working_dir, &mut |path| {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str())
                        && fname.ends_with(name)
                    {
                        matches.push(path.to_path_buf());
                    }
                });
            } else if pattern.contains('*') {
                // Prefix pattern like test_*.py
                let prefix = pattern.split('*').next().unwrap_or("");
                let suffix = pattern.split('*').nth(1).unwrap_or("");
                self.walk_files(&self.working_dir, &mut |path| {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str())
                        && fname.starts_with(prefix)
                        && fname.ends_with(suffix)
                    {
                        matches.push(path.to_path_buf());
                    }
                });
            } else {
                // Exact file name
                let path = self.working_dir.join(pattern);
                if path.exists() {
                    matches.push(path);
                }
            }
        }
        matches
    }

    fn walk_files(&self, dir: &Path, callback: &mut dyn FnMut(&Path)) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                continue;
            }
            if path.is_file() {
                callback(&path);
            } else if path.is_dir() {
                self.walk_files(&path, callback);
            }
        }
    }

    fn basic_structure(&self) -> String {
        match fs::read_dir(&self.working_dir) {
            Ok(entries) => {
                let mut names: Vec<String> = entries
                    .flatten()
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .collect();
                names.sort();
                let listing = names.join("\n");
                if listing.len() > 1200 {
                    let truncated: Vec<&str> = listing.lines().take(40).collect();
                    truncated.join("\n") + "\n... (truncated)"
                } else {
                    listing
                }
            }
            Err(_) => "(Unable to generate structure)".to_string(),
        }
    }
}

#[cfg(test)]
#[path = "indexer_tests.rs"]
mod tests;
