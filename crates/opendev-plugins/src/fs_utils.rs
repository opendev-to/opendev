use std::path::Path;

/// Helper function to atomically write a file with secure permissions (0o600).
pub fn atomic_write_secure(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp_name = format!(
        "{}.tmp.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    );
    let tmp_path = path.with_file_name(tmp_name);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(0o600);
        let mut file = opts.open(&tmp_path)?;
        std::io::Write::write_all(&mut file, content)?;
    }
    #[cfg(not(unix))]
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        let mut file = opts.open(&tmp_path)?;
        std::io::Write::write_all(&mut file, content)?;
    }

    std::fs::rename(&tmp_path, path)
}
