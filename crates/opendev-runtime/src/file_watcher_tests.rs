use super::*;
use tempfile::TempDir;

#[test]
fn test_should_ignore_git() {
    let patterns: Vec<String> = DEFAULT_IGNORE_DIRS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert!(should_ignore(Path::new("/project/.git/config"), &patterns));
    assert!(should_ignore(
        Path::new("/project/node_modules/pkg/index.js"),
        &patterns
    ));
    assert!(should_ignore(
        Path::new("/project/target/debug/binary"),
        &patterns
    ));
    assert!(should_ignore(
        Path::new("/project/.opendev/state.json"),
        &patterns
    ));
    assert!(!should_ignore(Path::new("/project/src/main.rs"), &patterns));
}

#[test]
fn test_file_watcher_config_default() {
    let config = FileWatcherConfig::default();
    assert_eq!(config.debounce, Duration::from_millis(500));
    assert_eq!(config.inactivity_timeout, Duration::from_secs(300));
    assert!(config.ignore_patterns.contains(&".git".to_string()));
    assert!(config.ignore_patterns.contains(&"target".to_string()));
    assert!(config.ignore_patterns.contains(&"node_modules".to_string()));
    assert!(config.ignore_patterns.contains(&".opendev".to_string()));
}

#[tokio::test]
#[ignore = "slow: macOS FSEvents debouncer cleanup takes ~24s"]
async fn test_file_watcher_start_and_stop() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    std::fs::write(tmp_path.join("test.txt"), "hello").unwrap();

    let watcher = FileWatcher::new(
        &tmp_path,
        FileWatcherConfig {
            debounce: Duration::from_millis(50),
            inactivity_timeout: Duration::from_millis(500),
            ..Default::default()
        },
    );

    let mut rx = watcher.start();

    // Give the watcher time to initialize
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create a new file to trigger a change
    std::fs::write(tmp_path.join("new.txt"), "new").unwrap();

    // Wait for the change to be detected
    let change = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(change.is_ok(), "Should receive a change event");

    // Stop the watcher
    watcher.stop();
}

#[tokio::test]
#[ignore = "slow: macOS FSEvents debouncer cleanup takes ~24s"]
async fn test_file_watcher_inactivity_timeout() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();

    let watcher = FileWatcher::new(
        &tmp_path,
        FileWatcherConfig {
            debounce: Duration::from_millis(50),
            inactivity_timeout: Duration::from_millis(500),
            ..Default::default()
        },
    );

    let mut rx = watcher.start();

    // Let initial FSEvents settle (macOS emits events on watch setup)
    tokio::time::sleep(Duration::from_millis(200)).await;
    while rx.try_recv().is_ok() {}

    // Wait for timeout — the channel should close
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        while rx.recv().await.is_some() {
            // drain events
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Watcher should stop after inactivity timeout"
    );
}
