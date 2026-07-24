use std::path::Path;
use std::time::SystemTime;

async fn scan_all_memory_files(dir: &Path) {
    let mut read_dir = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(_) => return,
    };

    let mut files = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.ends_with(".md") && name != "MEMORY.md" => name.to_string(),
            _ => continue,
        };

        let modified = entry
            .metadata()
            .await
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(_) => continue,
        };
    }
}
