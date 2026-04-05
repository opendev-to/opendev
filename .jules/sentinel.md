## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.
## 2025-02-14 - Fix insecure file permissions for sensitive MCP config
**Vulnerability:** The `opendev-mcp` crate was saving sensitive configuration (such as OAuth credentials and environment variables in `mcp.json`) using `std::fs::write` directly. This resulted in the file having default permissions (e.g., `0644`), making it readable by any other user on the system, which could lead to privilege escalation or data leakage.
**Learning:** Even internal tool configuration files, if they contain secrets or tokens, require strict file permissions to prevent TOCTOU vulnerabilities or unauthorized access by local actors.
**Prevention:** Always use a temporary file with restrictive permissions (0600 on Unix-like systems via `std::os::unix::fs::OpenOptionsExt::mode(0o600)`) followed by an atomic `std::fs::rename` when persisting configuration files that contain secrets.
