//! Just-in-time context retrieval for proactive code loading.
//!
//! Extracts entities (files, functions, classes) from user input and
//! resolves them against the filesystem to provide relevant context.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

/// Entities extracted from user input.
#[derive(Debug, Clone, Default)]
pub struct Entities {
    pub files: Vec<String>,
    pub functions: Vec<String>,
    pub classes: Vec<String>,
    pub variables: Vec<String>,
    pub actions: Vec<String>,
}

/// A file matched during context retrieval.
#[derive(Debug, Clone)]
pub struct FileMatch {
    pub path: String,
    pub reason: String,
    pub entity: String,
}

/// Result of a context retrieval operation.
#[derive(Debug, Clone, Default)]
pub struct RetrievalContext {
    pub entities: Entities,
    pub files_found: Vec<FileMatch>,
    pub suggestions: Vec<String>,
}

/// Extract entities (files, functions, classes, variables, actions) from user input.
#[derive(Debug)]
pub struct EntityExtractor {
    file_path_re: Regex,
    function_re: Regex,
    class_re: Regex,
    variable_re: Regex,
    action_re: Regex,
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityExtractor {
    /// Create a new entity extractor with pre-compiled regex patterns.
    pub fn new() -> Self {
        let extensions = [
            "py", "js", "ts", "jsx", "tsx", "java", "cpp", "c", "h", "hpp", "go", "rs", "rb",
            "php", "swift", "kt", "cs", "r", "m", "scala", "sh", "bash", "zsh", "yaml", "yml",
            "json", "toml", "xml", "html", "css", "scss", "sass", "md", "txt", "sql",
        ];
        let ext_pattern = extensions.join("|");
        let file_path_pattern = format!(r"[\w\-_./]+\.(?:{})", ext_pattern);

        Self {
            file_path_re: Regex::new(&file_path_pattern).unwrap(),
            function_re: Regex::new(r"(?:^|[^A-Z])([a-z_][a-z0-9_]*)\s*\(").unwrap(),
            class_re: Regex::new(
                r"\b([A-Z][a-zA-Z0-9]*(?:Error|Exception|Manager|Service|Handler|Controller|Model|View|Component)?)\b",
            )
            .unwrap(),
            variable_re: Regex::new(r"\b(?:var|let|const|self|this)\s+([a-z_][a-z0-9_]*)\b")
                .unwrap(),
            action_re: Regex::new(
                r"(?i)\b(fix|debug|implement|create|add|remove|delete|update|modify|refactor|test|check|verify|optimize)\b",
            )
            .unwrap(),
        }
    }

    /// Extract entities from user input text.
    pub fn extract_entities(&self, input: &str) -> Entities {
        let mut entities = Entities::default();

        // Files
        for cap in self.file_path_re.find_iter(input) {
            let val = cap.as_str().to_string();
            if !entities.files.contains(&val) {
                entities.files.push(val);
            }
        }

        // Functions
        for cap in self.function_re.captures_iter(input) {
            if let Some(m) = cap.get(1) {
                let val = m.as_str().to_string();
                if !entities.functions.contains(&val) {
                    entities.functions.push(val);
                }
            }
        }

        // Classes
        for cap in self.class_re.captures_iter(input) {
            if let Some(m) = cap.get(1) {
                let val = m.as_str().to_string();
                if !entities.classes.contains(&val) {
                    entities.classes.push(val);
                }
            }
        }

        // Variables
        for cap in self.variable_re.captures_iter(input) {
            if let Some(m) = cap.get(1) {
                let val = m.as_str().to_string();
                if !entities.variables.contains(&val) {
                    entities.variables.push(val);
                }
            }
        }

        // Actions
        for cap in self.action_re.captures_iter(input) {
            if let Some(m) = cap.get(1) {
                let val = m.as_str().to_lowercase();
                if !entities.actions.contains(&val) {
                    entities.actions.push(val);
                }
            }
        }

        entities
    }
}

/// Retrieve relevant context based on user intent.
#[derive(Debug)]
pub struct ContextRetriever {
    working_dir: PathBuf,
    extractor: EntityExtractor,
}

impl ContextRetriever {
    /// Create a new context retriever rooted at the given directory.
    pub fn new(working_dir: &Path) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
            extractor: EntityExtractor::new(),
        }
    }

    /// Retrieve context relevant to the user's input.
    ///
    /// Extracts entities, resolves file paths, searches for symbols,
    /// and generates suggestions based on detected actions.
    pub fn retrieve_context(&self, input: &str, max_files: usize) -> RetrievalContext {
        let entities = self.extractor.extract_entities(input);
        let mut ctx = RetrievalContext {
            entities: entities.clone(),
            files_found: Vec::new(),
            suggestions: Vec::new(),
        };

        // Resolve directly mentioned files
        for file_path in &entities.files {
            if let Some(resolved) = self.resolve_file_path(file_path) {
                ctx.files_found.push(FileMatch {
                    path: resolved.to_string_lossy().to_string(),
                    reason: "direct_mention".to_string(),
                    entity: file_path.clone(),
                });
            }
        }

        // Search for functions and classes
        let search_terms: Vec<&String> = entities
            .functions
            .iter()
            .chain(entities.classes.iter())
            .collect();

        for term in search_terms {
            let matches = self.grep_pattern(term, 5);
            for match_path in matches {
                let already_found = ctx.files_found.iter().any(|f| f.path == match_path);
                if !already_found {
                    ctx.files_found.push(FileMatch {
                        path: match_path,
                        reason: "contains_entity".to_string(),
                        entity: term.clone(),
                    });
                }
            }
        }

        // Generate suggestions based on actions
        if entities.actions.contains(&"fix".to_string())
            || entities.actions.contains(&"debug".to_string())
        {
            ctx.suggestions
                .push("Consider checking test files and error logs".to_string());
        }
        if entities.actions.contains(&"implement".to_string())
            || entities.actions.contains(&"create".to_string())
        {
            ctx.suggestions
                .push("Consider checking similar implementations".to_string());
        }

        ctx.files_found.truncate(max_files);
        ctx
    }

    /// Resolve a file path relative to the working directory.
    ///
    /// First checks the exact path, then searches recursively by filename.
    pub fn resolve_file_path(&self, file_path: &str) -> Option<PathBuf> {
        let path = self.working_dir.join(file_path);
        if path.exists() {
            return Some(path);
        }

        // Search by filename
        let target_name = Path::new(file_path).file_name()?.to_str()?;

        self.find_file_recursive(&self.working_dir, target_name)
    }

    /// Search for a pattern using `rg` (ripgrep), falling back to `grep`.
    pub fn grep_pattern(&self, pattern: &str, limit: usize) -> Vec<String> {
        // Try ripgrep first
        let result = Command::new("rg")
            .args(["-l", pattern])
            .arg(&self.working_dir)
            .output();

        let output = match result {
            Ok(output) if output.status.success() => output,
            _ => {
                // Fallback to grep
                match Command::new("grep")
                    .args(["-r", "-l", pattern])
                    .arg(&self.working_dir)
                    .output()
                {
                    Ok(output) if output.status.success() => output,
                    _ => return Vec::new(),
                }
            }
        };

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .take(limit)
            .map(|line| line.trim().to_string())
            .collect()
    }

    // -- Private helpers --

    fn find_file_recursive(&self, dir: &Path, target_name: &str) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                continue;
            }

            if path.is_file() {
                if name_str == target_name {
                    return Some(path);
                }
            } else if path.is_dir()
                && let Some(found) = self.find_file_recursive(&path, target_name)
            {
                return Some(found);
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "retriever_tests.rs"]
mod tests;
