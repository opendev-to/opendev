use super::*;
use std::fs;

#[test]
fn test_extract_file_entities() {
    let extractor = EntityExtractor::new();
    let entities = extractor.extract_entities("Please look at src/main.rs and config.toml");
    assert!(entities.files.contains(&"src/main.rs".to_string()));
    assert!(entities.files.contains(&"config.toml".to_string()));
}

#[test]
fn test_extract_function_entities() {
    let extractor = EntityExtractor::new();
    let entities = extractor.extract_entities("Call process_data() and validate_input()");
    assert!(entities.functions.contains(&"process_data".to_string()));
    assert!(entities.functions.contains(&"validate_input".to_string()));
}

#[test]
fn test_extract_class_entities() {
    let extractor = EntityExtractor::new();
    let entities = extractor.extract_entities("The UserManager and ErrorHandler classes");
    assert!(entities.classes.contains(&"UserManager".to_string()));
    assert!(entities.classes.contains(&"ErrorHandler".to_string()));
}

#[test]
fn test_extract_action_entities() {
    let extractor = EntityExtractor::new();
    let entities = extractor.extract_entities("Fix the bug and refactor the code");
    assert!(entities.actions.contains(&"fix".to_string()));
    assert!(entities.actions.contains(&"refactor".to_string()));
}

#[test]
fn test_extract_variable_entities() {
    let extractor = EntityExtractor::new();
    let entities = extractor.extract_entities("set let my_var = 10 and const count = 5");
    assert!(entities.variables.contains(&"my_var".to_string()));
    assert!(entities.variables.contains(&"count".to_string()));
}

#[test]
fn test_resolve_file_path_existing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.py"), "pass").unwrap();

    let retriever = ContextRetriever::new(dir.path());
    let resolved = retriever.resolve_file_path("test.py");
    assert!(resolved.is_some());
    assert!(resolved.unwrap().ends_with("test.py"));
}

#[test]
fn test_resolve_file_path_missing() {
    let dir = tempfile::tempdir().unwrap();
    let retriever = ContextRetriever::new(dir.path());
    assert!(retriever.resolve_file_path("nonexistent.py").is_none());
}

#[test]
fn test_retrieve_context_suggestions() {
    let dir = tempfile::tempdir().unwrap();
    let retriever = ContextRetriever::new(dir.path());

    let ctx = retriever.retrieve_context("Fix the broken login", 10);
    assert!(ctx.suggestions.iter().any(|s| s.contains("test files")));
    assert!(ctx.entities.actions.contains(&"fix".to_string()));
}

#[test]
fn test_retrieve_context_max_files() {
    let dir = tempfile::tempdir().unwrap();
    // Create many files
    for i in 0..5 {
        fs::write(dir.path().join(format!("file{}.py", i)), "pass").unwrap();
    }

    let retriever = ContextRetriever::new(dir.path());
    let ctx = retriever.retrieve_context("Look at file0.py file1.py file2.py", 2);
    assert!(ctx.files_found.len() <= 2);
}

#[test]
fn test_entities_default() {
    let entities = Entities::default();
    assert!(entities.files.is_empty());
    assert!(entities.functions.is_empty());
    assert!(entities.classes.is_empty());
    assert!(entities.variables.is_empty());
    assert!(entities.actions.is_empty());
}
