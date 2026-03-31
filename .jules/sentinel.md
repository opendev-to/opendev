## 2025-02-26 - [TOCTOU in Sensitive File Creation]
**Vulnerability:** Time-of-Check to Time-of-Use (TOCTOU) vulnerability where sensitive files (like `auth.json`) were created with default permissions using `std::fs::write` and then restricted using `std::fs::set_permissions(..., 0o600)`. This leaves a brief window where the file is readable by others.
**Learning:** Post-creation permission modification leaves a race condition window that can be exploited, especially for files storing API keys and credentials.
**Prevention:** Always use `std::fs::OpenOptions` with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to securely and atomically create the file with restricted permissions before writing any data to it.

## 2025-02-26 - [Overly Permissive File Permissions for Sensitive Data]
**Vulnerability:** File containing sensitive credentials, specifically the main `AppConfig` which can contain an `api_key`, was created using `std::fs::write` causing the file to be written with default permissions (typically 644 or 666 depending on umask). This allows other users on the system to potentially read the file and leak API keys.
**Learning:** Functions that write settings or configurations often include sensitive data (like `api_key`). Default file permissions in standard library functions (`std::fs::write`, `File::create`) do not enforce confidentiality against other users on the same Unix system.
**Prevention:** Whenever writing files that could contain configuration or credentials, use `std::fs::OpenOptions` along with `std::os::unix::fs::OpenOptionsExt::mode(0o600)` to ensure the created file is restricted only to the owner. This applies to initial writes as well as atomic temporary file writes.
