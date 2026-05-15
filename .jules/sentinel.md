## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-18 - Enforce Secure File Permissions via Atomic Writes for Configuration Files
**Vulnerability:** Configuration files containing sensitive data (like MCP OAuth client secrets or access tokens) were written using `std::fs::write` or non-atomic serialization. This creates a Time-of-Check to Time-of-Use (TOCTOU) race condition and defaults to standard user permissions, potentially allowing unauthorized read access on multi-user systems.
**Learning:** Directly modifying permissions after writing a file still leaves a short window where a local attacker can read or modify the file.
**Prevention:** Always write sensitive files using an atomic pattern: create a temporary file using `std::fs::OpenOptions` with `.create(true).write(true).truncate(true).mode(0o600)` (on Unix via `std::os::unix::fs::OpenOptionsExt`), write the contents, and then use `std::fs::rename` to atomically replace the destination file.

## 2025-02-27 - [TOCTOU in State Snapshot Persistence]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive application state snapshots were created using `std::fs::write`, which defaults to standard user permissions, before being renamed. Additionally, predictable temporary file names and path traversal risks via unsanitized `session_id` were present.
**Learning:** Even internal snapshot or recovery files containing sensitive state (like tokens or session info) must use atomic write patterns with secure permissions to prevent local privilege escalation or information disclosure.
**Prevention:** Always use `uuid::Uuid::new_v4()` for temporary file names, `std::fs::OpenOptions` with `.create_new(true).write(true)`, and `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely create snapshot files before renaming them. Sanitize any user-controlled input (like `session_id`) used in paths.
