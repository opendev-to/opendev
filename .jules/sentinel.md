## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-18 - Enforce Secure File Permissions via Atomic Writes for Configuration Files
**Vulnerability:** Configuration files containing sensitive data (like MCP OAuth client secrets or access tokens) were written using `std::fs::write` or non-atomic serialization. This creates a Time-of-Check to Time-of-Use (TOCTOU) race condition and defaults to standard user permissions, potentially allowing unauthorized read access on multi-user systems.
**Learning:** Directly modifying permissions after writing a file still leaves a short window where a local attacker can read or modify the file.
**Prevention:** Always write sensitive files using an atomic pattern: create a temporary file using `std::fs::OpenOptions` with `.create(true).write(true).truncate(true).mode(0o600)` (on Unix via `std::os::unix::fs::OpenOptionsExt`), write the contents, and then use `std::fs::rename` to atomically replace the destination file.

## 2025-02-27 - [TOCTOU in State Snapshot Creation]
**Vulnerability:** State snapshots in `crates/opendev-runtime/src/state_snapshot.rs` were created using a temporary file with default permissions via `std::fs::write(&tmp_path, &json)` and then renamed. This created a Time-of-Check to Time-of-Use (TOCTOU) window where sensitive data (like environment variables, access tokens, or code snippets in the snapshot) could be read by unauthorized local users.
**Learning:** State snapshot files often contain just as much sensitive data as configuration files or authentication caches. They must be secured using the exact same atomic write patterns.
**Prevention:** Always use the secure atomic write pattern for any state snapshot persistence: use an `#[cfg(unix)]` block with `std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp_path)` to ensure the temporary file is created with restrictive permissions before any data is written to it.
