## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2024-05-30 - [Secure Config File Permissions]
**Vulnerability:** Configuration files (`mcp.json`) which contain sensitive settings like API keys and system environment variables, were being written using `std::fs::write` directly. This creates a file with default permissions, which allows other users to read sensitive credentials.
**Learning:** Directly writing to file paths for security configurations results in insecure default permissions. To prevent TOCTOU vulnerabilities and securely save credentials, we need to apply atomic writes alongside tightly restricted file permissions (`0o600`).
**Prevention:** In Rust environments, instead of utilizing standard `std::fs::write`, write configuration data securely to a temporary file via `.create_new(true)` with `.mode(0o600)` permissions via `std::os::unix::fs::OpenOptionsExt` (on Unix systems) and then apply `std::fs::rename` to move the temporary file exactly into place without a TOCTOU vulnerable window.
