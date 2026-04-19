## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-18 - Enforce Secure File Permissions via Atomic Writes for Configuration Files
**Vulnerability:** Configuration files containing sensitive data (like MCP OAuth client secrets or access tokens) were written using `std::fs::write` or non-atomic serialization. This creates a Time-of-Check to Time-of-Use (TOCTOU) race condition and defaults to standard user permissions, potentially allowing unauthorized read access on multi-user systems.
**Learning:** Directly modifying permissions after writing a file still leaves a short window where a local attacker can read or modify the file.
**Prevention:** Always write sensitive files using an atomic pattern: create a temporary file using `std::fs::OpenOptions` with `.create(true).write(true).truncate(true).mode(0o600)` (on Unix via `std::os::unix::fs::OpenOptionsExt`), write the contents, and then use `std::fs::rename` to atomically replace the destination file.

## 2025-02-26 - [TOCTOU in State Snapshot Recovery Files]
**Vulnerability:** Application state snapshots (which may contain sensitive tool outputs or path data) were written using `std::fs::write` to a predictable temporary filename (`.json.tmp`) before being renamed. This created a TOCTOU (Time-of-Check to Time-of-Use) vulnerability with default file permissions, exposing the temporary file to unauthorized reads on multi-user systems.
**Learning:** Using `std::fs::write` for any sensitive temporary file is insecure because it relies on default umasks. Predictable temp filenames also increase the likelihood of race condition exploits.
**Prevention:** Always write sensitive files using an atomic pattern with unpredictable filenames (e.g., `uuid::Uuid::new_v4()`) and securely restricted permissions during creation (via `OpenOptionsExt::mode(0o600)` and `create_new(true)`).
