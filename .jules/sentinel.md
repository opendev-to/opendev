## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-18 - Enforce Secure File Permissions via Atomic Writes for Configuration Files
**Vulnerability:** Configuration files containing sensitive data (like MCP OAuth client secrets or access tokens) were written using `std::fs::write` or non-atomic serialization. This creates a Time-of-Check to Time-of-Use (TOCTOU) race condition and defaults to standard user permissions, potentially allowing unauthorized read access on multi-user systems.
**Learning:** Directly modifying permissions after writing a file still leaves a short window where a local attacker can read or modify the file.
**Prevention:** Always write sensitive files using an atomic pattern: create a temporary file using `std::fs::OpenOptions` with `.create(true).write(true).truncate(true).mode(0o600)` (on Unix via `std::os::unix::fs::OpenOptionsExt`), write the contents, and then use `std::fs::rename` to atomically replace the destination file.

## 2025-05-14 - Fix TOCTOU vulnerability in Application State Snapshots
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where `AppStateSnapshot` data was written via `std::fs::write(&tmp_path, &json)` before renaming. This could allow unauthorized access to sensitive application state on multi-user systems since default permissions were used, and atomic writing alone does not secure the temp file itself during writing.
**Learning:** Even short-lived temporary files used for atomic renaming can be intercepted or read during the write process if they contain sensitive system state (like API keys or session information) and are created with default broad permissions.
**Prevention:** Always write sensitive state serialization using an atomic pattern with `.create_new(true)` and `.mode(0o600)` (on Unix) via `std::fs::OpenOptions` instead of `std::fs::write`, to securely enforce read/write restrictions before data touches the disk.

## 2025-05-16 - [Fix TOCTOU vulnerability in Configuration Atomic Writes]
**Vulnerability:** Across various configuration files (`auth.rs`, `user_store.rs`, `mcp/io.rs`, `config.rs`, `loader/mod.rs`), atomic writes utilized `.create(true).truncate(true)` instead of `.create_new(true)`. On non-Unix platforms, the fallback utilized `std::fs::write(&tmp_path, ...)`. This resulted in a Time-of-Check to Time-of-Use (TOCTOU) vulnerability where an attacker could theoretically predict the randomized temporary filename or exploit symlinks to intercept sensitive configuration material.
**Learning:** Even when employing randomized UUIDs for temporary file names within atomic write patterns, utilizing `create(true)` or `std::fs::write` leaves a theoretical attack surface for unauthorized reading and symlink attacks.
**Prevention:** Unconditionally employ `.create_new(true)` to securely enforce that temporary files are exclusively created afresh by the process, failing securely if the path already exists or has been externally manipulated. Apply this standard universally to both Unix and non-Unix write blocks when persisting sensitive settings or application state.

## 2025-05-17 - Path Traversal in state_snapshot.rs
**Vulnerability:** A path traversal vulnerability existed in `crates/opendev-runtime/src/state_snapshot.rs` where the `session_id` was directly substituted into the snapshot filename without sanitization. An attacker or bug could supply a malformed `session_id` containing `../` characters, resulting in files being written or read outside of the intended directory.
**Learning:** File paths dynamically constructed using unsanitized user inputs or unbounded strings without proper normalization can lead to arbitrary file reads or writes. Even internally generated IDs should be sanitized if they are part of a file path, especially if there's potential for external contamination or test data with non-standard formats.
**Prevention:** Always sanitize components of a file path derived from external or semi-external data by replacing or stripping path separator characters (like `/` and `\`) before constructing the final path.
