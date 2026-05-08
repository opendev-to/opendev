## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-18 - Enforce Secure File Permissions via Atomic Writes for Configuration Files
**Vulnerability:** Configuration files containing sensitive data (like MCP OAuth client secrets or access tokens) were written using `std::fs::write` or non-atomic serialization. This creates a Time-of-Check to Time-of-Use (TOCTOU) race condition and defaults to standard user permissions, potentially allowing unauthorized read access on multi-user systems.
**Learning:** Directly modifying permissions after writing a file still leaves a short window where a local attacker can read or modify the file.
**Prevention:** Always write sensitive files using an atomic pattern: create a temporary file using `std::fs::OpenOptions` with `.create(true).write(true).truncate(true).mode(0o600)` (on Unix via `std::os::unix::fs::OpenOptionsExt`), write the contents, and then use `std::fs::rename` to atomically replace the destination file.

## 2025-05-08 - Use create_new(true) for Secure File Creation
**Vulnerability:** Even when using `OpenOptions` with restricted permissions (e.g., `mode(0o600)`), using `.create(true).truncate(true)` instead of `.create_new(true)` can be exploited via symlink attacks if the temporary filename is predictable (e.g., just appending `.tmp` instead of a UUID). An attacker could create a symlink with the predictable name before the application writes to it, causing the application to truncate and overwrite an arbitrary file.
**Learning:** Atomic file replacement requires two guarantees: 1) the temporary filename must be unpredictable, and 2) the temporary file must be created exclusively without following symlinks.
**Prevention:** Always use `uuid::Uuid::new_v4()` for temporary filenames during atomic writes, and use `opts.write(true).create_new(true)` instead of `opts.write(true).create(true).truncate(true)` to ensure the file doesn't already exist and symlinks are not followed.
