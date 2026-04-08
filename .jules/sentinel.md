## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.
## 2025-04-08 - Insecure File Permissions on MCP Configurations
**Vulnerability:** The MCP configuration files (`mcp.json`), which can contain sensitive `oauth` client secrets, were being written using `std::fs::write`, which relies on the system's default `umask` (often `0644`). This allowed the sensitive secrets to be read by any local user on the machine.
**Learning:** `std::fs::write` does not allow specifying file permissions on creation, making it unsuitable for writing sensitive configuration files or credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `.mode(0o600)` (on Unix) via `std::os::unix::fs::OpenOptionsExt` when writing files that contain sensitive configurations, combined with an atomic rename to prevent TOCTOU races.
