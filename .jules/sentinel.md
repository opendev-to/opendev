## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2024-05-18 - Atomic writes for MCP Configuration
**Vulnerability:** MCP configuration files were being written directly to their target paths without temporary files and without restrictive permissions. This exposed sensitive config information (such as OAuth keys and tokens) and created TOCTOU vulnerabilities as well as the risk of configuration corruption if the write failed midway.
**Learning:** Configurations often hold secrets and must be written securely. We should always use an atomic file rename combined with restrictive permissions to ensure secrets are safe from local unauthorized reads and file writes don't corrupt the file.
**Prevention:** Use an atomic write pattern (`OpenOptionsExt::mode(0o600)` -> write to temp file -> rename to target file) for any configuration files, especially ones holding keys and credentials, to ensure security and prevent data corruption.
