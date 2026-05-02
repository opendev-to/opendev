## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-18 - Enforce Secure File Permissions via Atomic Writes for Configuration Files
**Vulnerability:** Configuration files containing sensitive data (like MCP OAuth client secrets or access tokens) were written using `std::fs::write` or non-atomic serialization. This creates a Time-of-Check to Time-of-Use (TOCTOU) race condition and defaults to standard user permissions, potentially allowing unauthorized read access on multi-user systems.
**Learning:** Directly modifying permissions after writing a file still leaves a short window where a local attacker can read or modify the file.
**Prevention:** Always write sensitive files using an atomic pattern: create a temporary file using `std::fs::OpenOptions` with `.create(true).write(true).truncate(true).mode(0o600)` (on Unix via `std::os::unix::fs::OpenOptionsExt`), write the contents, and then use `std::fs::rename` to atomically replace the destination file.

## 2025-05-02 - Secure Atomic File Writing with create_new
**Vulnerability:** When writing sensitive configuration files (like `auth.json`) atomically, using predictable temporary extensions (like `.tmp`) combined with `std::fs::OpenOptions` `.create(true).truncate(true)` makes the operation vulnerable to symlink attacks or TOCTOU exploitation by other users on a shared system.
**Learning:** Even if `mode(0o600)` is specified, `.create(true)` allows an attacker to pre-create a symlink at the predictable `.tmp` location, causing the write to follow the symlink and overwrite an unintended file.
**Prevention:** Always use random extensions (e.g. `uuid::Uuid::new_v4()`) for temporary files and strictly use `.create_new(true)` with `std::fs::OpenOptions` so that the atomic write safely fails if the temporary file already exists (preventing symlink attacks or accidental overwrites).
