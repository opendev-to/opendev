use super::*;
use std::fs;
use tempfile::TempDir;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_list_files_basic() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.rs"), "").unwrap();
    fs::write(tmp.path().join("b.rs"), "").unwrap();
    fs::write(tmp.path().join("c.txt"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("*.rs"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("a.rs"));
    assert!(output.contains("b.rs"));
    assert!(!output.contains("c.txt"));
}

#[tokio::test]
async fn test_list_files_recursive() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), "").unwrap();
    fs::write(tmp.path().join("src/sub/lib.rs"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("**/*.rs"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("main.rs"));
    assert!(output.contains("lib.rs"));
}

#[tokio::test]
async fn test_list_files_max_depth() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("a/b/c")).unwrap();
    fs::write(tmp.path().join("top.rs"), "").unwrap();
    fs::write(tmp.path().join("a/mid.rs"), "").unwrap();
    fs::write(tmp.path().join("a/b/deep.rs"), "").unwrap();
    fs::write(tmp.path().join("a/b/c/deeper.rs"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());

    // max_depth 0 = only files in base dir
    let args = make_args(&[
        ("pattern", serde_json::json!("**/*.rs")),
        ("max_depth", serde_json::json!(0)),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("top.rs"));
    assert!(!output.contains("mid.rs"));
    assert!(!output.contains("deep.rs"));

    // max_depth 1 = base dir + one level
    let args = make_args(&[
        ("pattern", serde_json::json!("**/*.rs")),
        ("max_depth", serde_json::json!(1)),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("top.rs"));
    assert!(output.contains("mid.rs"));
    assert!(!output.contains("deep.rs"));

    // No max_depth = all files
    let args = make_args(&[("pattern", serde_json::json!("**/*.rs"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("top.rs"));
    assert!(output.contains("mid.rs"));
    assert!(output.contains("deep.rs"));
    assert!(output.contains("deeper.rs"));
}

#[tokio::test]
async fn test_list_files_excludes_node_modules() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::create_dir_all(tmp.path().join("node_modules/foo")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(
        tmp.path().join("node_modules/foo/index.js"),
        "module.exports = {}",
    )
    .unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("**/*"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("main.rs"));
    assert!(
        !output.contains("index.js"),
        "node_modules should be excluded"
    );
}

#[tokio::test]
async fn test_list_files_excludes_build_dirs() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::create_dir_all(tmp.path().join("target/debug")).unwrap();
    fs::create_dir_all(tmp.path().join("__pycache__")).unwrap();
    fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    fs::write(tmp.path().join("target/debug/output"), "").unwrap();
    fs::write(tmp.path().join("__pycache__/mod.pyc"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("**/*"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("lib.rs"));
    assert!(!output.contains("output"), "target/ should be excluded");
    assert!(
        !output.contains("mod.pyc"),
        "__pycache__/ should be excluded"
    );
}

#[tokio::test]
async fn test_list_files_excludes_minified_files() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("app.js"), "").unwrap();
    fs::write(tmp.path().join("app.min.js"), "").unwrap();
    fs::write(tmp.path().join("style.min.css"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("*"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("app.js"));
    assert!(
        !output.contains("app.min.js"),
        "*.min.js should be excluded"
    );
    assert!(
        !output.contains("style.min.css"),
        "*.min.css should be excluded"
    );
}

#[test]
fn test_is_excluded_path() {
    assert!(is_excluded_path(Path::new("node_modules/foo/bar.js")));
    assert!(is_excluded_path(Path::new("src/vendor/lib.go")));
    assert!(is_excluded_path(Path::new(".git/HEAD")));
    assert!(is_excluded_path(Path::new("dist/bundle.js")));
    assert!(is_excluded_path(Path::new("app.min.js")));
    assert!(is_excluded_path(Path::new("style.min.css")));
    assert!(!is_excluded_path(Path::new("src/main.rs")));
    assert!(!is_excluded_path(Path::new("lib.rs")));
}

#[tokio::test]
async fn test_list_files_custom_ignore() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("logs")).unwrap();
    fs::write(tmp.path().join("app.rs"), "").unwrap();
    fs::write(tmp.path().join("debug.log"), "").unwrap();
    fs::write(tmp.path().join("logs/app.log"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("pattern", serde_json::json!("**/*")),
        ("ignore", serde_json::json!(["*.log", "logs/"])),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("app.rs"));
    assert!(!output.contains("debug.log"), "*.log should be ignored");
    assert!(!output.contains("app.log"), "logs/ dir should be ignored");
}

#[tokio::test]
async fn test_list_files_no_matches() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("*.rs"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("No files found"));
}

#[tokio::test]
async fn test_list_files_missing_dir_shows_available() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    fs::create_dir_all(tmp_path.join("crates")).unwrap();
    fs::create_dir_all(tmp_path.join("docs")).unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(&tmp_path);
    let args = make_args(&[
        ("pattern", serde_json::json!("**/*.rs")),
        (
            "path",
            serde_json::json!(tmp_path.join("src").to_str().unwrap()),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("Directory not found"), "got: {err}");
    assert!(err.contains("crates/"), "should list crates/, got: {err}");
    assert!(err.contains("docs/"), "should list docs/, got: {err}");
}

#[tokio::test]
async fn test_list_files_nonexistent_pattern_dir_shows_hint() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    fs::create_dir_all(tmp_path.join("crates")).unwrap();
    fs::write(tmp_path.join("crates/lib.rs"), "").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(&tmp_path);
    let args = make_args(&[("pattern", serde_json::json!("src/**/*.rs"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(
        output.contains("does not exist"),
        "should note src/ doesn't exist, got: {output}"
    );
    assert!(
        output.contains("crates/"),
        "should suggest crates/, got: {output}"
    );
}
