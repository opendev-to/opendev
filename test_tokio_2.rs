use std::path::Path;
use tokio::fs;

async fn test(dir: &Path) {
    let mut entries = fs::read_dir(dir).await.unwrap();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let md = entry.metadata().await.unwrap();
        let modified = md.modified().unwrap();
        let path = entry.path();
    }
}
