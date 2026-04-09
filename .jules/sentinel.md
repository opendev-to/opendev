## 2025-04-09 - TOCTOU vulnerabilities in config persistence
**Vulnerability:** `mcp.json` (which contains MCP server OAuth config and secrets) was being saved using `std::fs::write`, which could allow malicious local processes to read or overwrite the config briefly before or during writes.
**Learning:** Security-sensitive files must be written securely using temporary files with restrictive permissions (0o600 on UNIX) via `std::os::unix::fs::OpenOptionsExt`, then atomically renamed. `uuid` should be used for unique tmp files.
**Prevention:** Always check new occurrences of `std::fs::write` on configuration files or user files containing sensitive configuration. Use the atomic rename pattern with `OpenOptions`.

## 2025-04-09 - TOCTOU vulnerabilities in config persistence
**Vulnerability:** `mcp.json` (which contains MCP server OAuth config and secrets) was being saved using `std::fs::write`, which could allow malicious local processes to read or overwrite the config briefly before or during writes.
**Learning:** Security-sensitive files must be written securely using temporary files with restrictive permissions (0o600 on UNIX) via `std::os::unix::fs::OpenOptionsExt`, then atomically renamed.
**Prevention:** Always check new occurrences of `std::fs::write` on configuration files or user files containing sensitive configuration. Use the atomic rename pattern with `OpenOptions` or `tempfile`.
