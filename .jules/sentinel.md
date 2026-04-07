## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.
## 2024-12-16 - Insecure File Permissions in Configuration Saves
**Vulnerability:** The `save_config` function in `crates/opendev-mcp/src/config.rs` used `std::fs::write` to save MCP configurations, which could contain sensitive OAuth credentials, without explicitly enforcing secure file permissions. It was also not atomic.
**Learning:** Using default file creation APIs can lead to files with sensitive content being accessible by other users on the system if default umask settings are permissive. In addition, direct writing to the target path is prone to partial writes.
**Prevention:** Use an atomic write pattern with a temporary file, and on Unix-like systems, set explicitly restrictive permissions `0o600` via `OpenOptionsExt` before renaming the file to the final destination.
