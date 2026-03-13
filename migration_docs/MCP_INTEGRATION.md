# MCP Integration

## Overview

The MCP (Model Context Protocol) integration enables OpenDev to connect to external tool servers using the standardized JSON-RPC 2.0 protocol. MCP servers expose tools, resources, and prompts that OpenDev can discover and invoke at runtime, extending the agent's capabilities without modifying the core codebase. The integration lives in the `opendev-mcp` crate (Rust) and `opendev/core/context_engineering/mcp/` package (Python), sitting between the tool registry (`opendev-tools-core`) and external MCP server processes.

## Python Architecture

### Module Structure

```
opendev/core/context_engineering/mcp/
  __init__.py              # Re-exports MCPManager, MCPServerConfig, MCPConfig
  config.py                # Config loading, saving, merging, env var expansion
  handler.py               # McpToolHandler - bridges tool dispatch to MCPManager
  models.py                # Pydantic models: MCPServerConfig, MCPConfig
  manager/
    __init__.py             # Re-exports MCPManager
    manager.py              # MCPManager class (mixin composition), event loop infra
    connection.py           # ConnectionMixin - connect/disconnect/discover lifecycle
    server_config.py        # ServerConfigMixin - CRUD for server configurations
    transport.py            # TransportMixin - transport factory (stdio/SSE/HTTP)
```

### Class Hierarchy

`MCPManager` is composed via Python mixins:

```
MCPManager(TransportMixin, ConnectionMixin, ServerConfigMixin)
```

- **TransportMixin** (`transport.py`): Factory method `_create_transport_from_config()` that maps server config to fastmcp transport classes (`NpxStdioTransport`, `NodeStdioTransport`, `PythonStdioTransport`, `UvxStdioTransport`, `StdioTransport`, `StreamableHttpTransport`, `SSETransport`). Handles command-specific dispatch (npx, node, python, uv, uvx, docker).
- **ConnectionMixin** (`connection.py`): Async internals (`_connect_internal`, `_disconnect_internal`, `_discover_tools`, `_call_tool_internal`) plus synchronous wrappers (`connect_sync`, `call_tool_sync`, etc.) that use a shared background event loop. Provides `connect_enabled_servers_background()` for non-blocking startup.
- **ServerConfigMixin** (`server_config.py`): CRUD operations for server configuration (`add_server`, `remove_server`, `enable_server`, `disable_server`, `list_servers`). Persists changes via `save_config()`.
- **McpToolHandler** (`handler.py`): Thin adapter that parses the `mcp__server__tool` naming convention, checks connection state, and delegates to `MCPManager.call_tool_sync()`.

### Key Abstractions

- **fastmcp.Client**: The Python implementation delegates all protocol work (initialize handshake, tool discovery, tool calls) to the third-party `fastmcp` library. OpenDev's code only manages transport selection, lifecycle, and result extraction.
- **Background event loop**: `MCPManager` runs a dedicated `asyncio` event loop in a daemon thread. All async MCP operations are scheduled via `asyncio.run_coroutine_threadsafe()`, with synchronous wrappers that block on `future.result(timeout=...)`. `call_tool_sync` polls with 100ms intervals to support interrupt checking via `TaskMonitor`.
- **`_SuppressStderr`**: Context manager that redirects fd 2 to `/dev/null` during connection/disconnection to hide noisy MCP server stderr output.

### Design Patterns

- **Mixin composition**: `MCPManager` splits responsibilities across three mixins for separation of concerns, though this creates implicit coupling through shared `self.clients` and `self.server_tools` dictionaries.
- **Sync-over-async bridge**: Every async method has a `_sync` counterpart. The event loop thread is lazily created and guarded by `threading.Lock` / `threading.Event`.
- **Per-server locking**: `_server_locks` dictionary prevents concurrent connect/disconnect attempts to the same server.

### SOLID Analysis

- **SRP**: Mostly adhered to via mixins, though `ConnectionMixin` handles both lifecycle and tool invocation.
- **OCP**: Transport types are extensible via `_create_transport_from_config()` factory, though adding a new type requires modifying the if/elif chain.
- **LSP**: All fastmcp transport classes are interchangeable from the Client's perspective.
- **ISP**: The mixin approach provides implicit interface segregation, but consumers always get the full `MCPManager` surface.
- **DIP**: `MCPManager` depends on the `fastmcp.Client` abstraction rather than concrete transport implementations.

## Rust Architecture

### Module Structure

```
crates/opendev-mcp/src/
  lib.rs          # Module declarations and public re-exports
  config.rs       # McpConfig, McpServerConfig, TransportType, load/save/merge/expand
  models.rs       # Protocol types: McpTool, McpToolResult, McpContent, JSON-RPC messages
  transport.rs    # McpTransport trait + StdioTransport, HttpTransport, SseTransport
  manager.rs      # McpManager - async coordinator with RwLock-guarded state
  error.rs        # McpError enum (thiserror) and McpResult type alias
```

### Trait Hierarchy

```rust
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn connect(&mut self) -> McpResult<()>;
    async fn send_request(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse>;
    async fn send_notification(&self, notification: &JsonRpcNotification) -> McpResult<()>;
    async fn close(&self) -> McpResult<()>;
    fn is_connected(&self) -> bool;
    fn transport_type(&self) -> &str;
}
```

Implemented by:
- **`StdioTransport`**: Spawns a child process, communicates via Content-Length framed JSON-RPC over stdin/stdout. Uses a background Tokio task for reading responses and `oneshot` channels for request/response correlation.
- **`HttpTransport`**: Stateless HTTP POST via `reqwest::Client`. No persistent connection.
- **`SseTransport`**: Currently uses HTTP POST (full SSE streaming noted as future work).

### Key Abstractions

- **`McpManager`**: Owns configuration (`Arc<RwLock<Option<McpConfig>>>`), active connections (`Arc<RwLock<HashMap<String, ServerConnection>>>`), and an atomic request ID counter. All methods are `async` with no sync-over-async bridge needed since Rust consumers are already in a Tokio runtime.
- **`ServerConnection`**: Internal struct holding a `Box<dyn McpTransport>`, discovered `Vec<McpTool>`, and the prepared config.
- **`McpToolSchema`**: Namespaced tool representation (`server_name__tool_name`) exposed to the LLM tool registry.
- **`McpError`**: Rich error enum with variants for Transport, Connection, ServerNotFound, AlreadyConnected, Config, Protocol, Timeout, IO, JSON, and HTTP errors.

### Design Patterns

- **Trait objects for transport polymorphism**: `Box<dyn McpTransport>` replaces the Python mixin-based transport dispatch. The `create_transport()` factory function maps `TransportType` enum to concrete implementations.
- **Interior mutability via `Arc<RwLock<_>>`**: Replaces Python's threading.Lock + mutable dictionaries. The `McpManager` is designed to be shared across tasks via `Arc`.
- **Content-Length framing**: The stdio transport implements the LSP-style Content-Length header protocol directly, parsing headers line-by-line and reading exact byte counts. The Python side delegates this to fastmcp.
- **Request/response correlation**: `StdioTransport` uses `HashMap<u64, oneshot::Sender<JsonRpcResponse>>` for matching responses to pending requests by ID, with a background reader task dispatching incoming messages.

### SOLID Analysis

- **SRP**: Clean separation: `config.rs` handles config, `transport.rs` handles communication, `manager.rs` handles coordination, `models.rs` handles data types, `error.rs` handles errors.
- **OCP**: Adding a new transport requires implementing `McpTransport` trait and adding a match arm in `create_transport()`. The trait-based approach is more extensible than Python's if/elif chain.
- **LSP**: All `McpTransport` implementations are fully substitutable; the manager only interacts through the trait interface.
- **ISP**: The `McpTransport` trait is focused (6 methods). `McpManager` exposes a clean public API without leaking internal state.
- **DIP**: `McpManager` depends on `dyn McpTransport` rather than concrete transport types.

## Migration Mapping

| Python Class/Module | Rust Struct/Trait | Pattern Change | Notes |
|---|---|---|---|
| `MCPConfig` (Pydantic) | `McpConfig` (serde) | Pydantic BaseModel to serde Derive | `alias="mcpServers"` becomes `serde(alias = "mcpServers")` |
| `MCPServerConfig` (Pydantic) | `McpServerConfig` (serde) | Pydantic Field defaults to serde defaults | `transport: str` becomes `transport: TransportType` enum |
| `TransportMixin._create_transport_from_config()` | `transport::create_transport()` | Mixin method to free function | Returns `Box<dyn McpTransport>` instead of fastmcp transport objects |
| `TransportMixin._create_stdio_transport()` | `transport::create_stdio_transport()` | if/elif chain to match + validation | Simplified: single `StdioTransport` for all commands (no NpxStdioTransport, etc.) |
| `ConnectionMixin` | `McpManager` methods | Mixin merged into manager | `connect_server()`, `disconnect_server()`, `connect_all()` |
| `ConnectionMixin._discover_tools()` | `McpManager::discover_tools()` | fastmcp `client.list_tools()` to raw JSON-RPC `tools/list` | Rust sends JSON-RPC directly, no third-party MCP client library |
| `ConnectionMixin._connect_internal()` | `McpManager::connect_server()` | Implicit fastmcp handshake to explicit `initialize_handshake()` | Rust implements the full MCP init protocol: `initialize` request + `notifications/initialized` |
| `ConnectionMixin._call_tool_internal()` | `McpManager::call_tool()` | fastmcp `client.call_tool()` to raw JSON-RPC `tools/call` | Returns typed `McpToolResult` instead of dict |
| `ServerConfigMixin` | `McpManager` methods | Mixin merged into manager | `add_server()`, `remove_server()` |
| `McpToolHandler` | (integrated in tools-impl) | Separate class to inline dispatch | Tool name parsing (`mcp__server__tool`) handled at tool registry level |
| `MCPManager._run_event_loop()` / `_ensure_event_loop()` | Not needed | Sync-over-async bridge eliminated | Rust callers are already async (Tokio runtime) |
| `MCPManager._run_coroutine_threadsafe()` | Not needed | Threading bridge eliminated | No background event loop thread needed |
| `_SuppressStderr` | stderr piped to `tracing::trace!` | fd redirect to structured logging | Rust captures stderr via `tokio::process::Stdio::piped()` and logs lines at trace level |
| `config.expand_env_vars()` | `config::expand_env_vars()` | `re.sub` to `regex::Regex::replace_all` | Same `${VAR}` syntax, same fallback-to-original behavior |
| `config.merge_configs()` | `config::merge_configs()` | dict.update to HashMap.extend | Same project-over-global precedence |
| `config.load_config()` / `save_config()` | `config::load_config()` / `save_config()` | json.load/dump to serde_json | Error handling via `McpResult` instead of try/except with print |
| `models.MCPConfig.model_config` | `#[serde(alias)]` | Pydantic ConfigDict to serde attribute | `populate_by_name=True` becomes `#[serde(alias = "mcpServers")]` |

## Protocol Details

### JSON-RPC 2.0 Message Format

MCP uses JSON-RPC 2.0 as its wire protocol. Three message types:

**Request** (client to server, expects response):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}
```

**Response** (server to client):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "tools": [...] }
}
```
Or with error:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": { "code": -32601, "message": "Method not found" }
}
```

**Notification** (no id, no response expected):
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized"
}
```

Rust models: `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcNotification`, `JsonRpcError` in `models.rs`.

### Transport Types and Their Implementations

**Stdio** (default): Spawns a child process. Communication uses Content-Length header framing over stdin/stdout:
```
Content-Length: 42\r\n
\r\n
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
```
- Python: Delegates to fastmcp transport classes (`NpxStdioTransport`, `NodeStdioTransport`, `PythonStdioTransport`, `UvxStdioTransport`, `StdioTransport`), each specialized for a command type.
- Rust: Single `StdioTransport` struct for all commands. Background Tokio task reads responses and dispatches via `oneshot` channels keyed by request ID. Stderr is captured and logged at trace level.

**HTTP** (Streamable HTTP): Stateless POST requests to a remote URL. Each JSON-RPC message is a separate HTTP request.
- Python: Uses fastmcp `StreamableHttpTransport`.
- Rust: `HttpTransport` wraps `reqwest::Client` with custom headers and timeout.

**SSE** (Server-Sent Events): Intended for long-lived server-push connections. Currently both Python and Rust implement this as HTTP POST (full SSE streaming is a remaining gap in Rust).
- Python: Uses fastmcp `SSETransport`.
- Rust: `SseTransport` wraps `reqwest::Client`, functionally identical to `HttpTransport`.

### Server Lifecycle Management

The connection lifecycle in Rust (`McpManager::connect_server()`):

1. **Config lookup**: Find `McpServerConfig` by name, verify `enabled` flag, check not already connected.
2. **Env var expansion**: `prepare_server_config()` expands `${VAR}` patterns in args, env, url, and headers.
3. **Transport creation**: `create_transport()` factory dispatches on `TransportType` enum.
4. **Transport connect**: For stdio, spawns child process. For HTTP/SSE, no-op (stateless).
5. **Initialize handshake**: Sends `initialize` request with `protocolVersion`, `capabilities`, and `clientInfo`. Waits for server response. Then sends `notifications/initialized` notification.
6. **Tool discovery**: Sends `tools/list` request, parses response into `Vec<McpTool>`.
7. **Registration**: Stores `ServerConnection` in the connections map.

Disconnection: closes transport (kills child process for stdio, waits up to 2 seconds, then force-kills), removes from connections map.

**Python difference**: Steps 4-6 are handled implicitly by `fastmcp.Client.__aenter__()` and `client.list_tools()`. The Rust implementation handles the protocol directly with explicit JSON-RPC messages.

## Key Design Decisions

1. **No fastmcp dependency in Rust**: The Python side uses fastmcp as a third-party MCP client library. The Rust side implements the MCP protocol from scratch using raw JSON-RPC messages over the transport layer. This gives full control over the protocol, eliminates a dependency, and enables Rust-idiomatic error handling, but means the Rust code must maintain protocol compatibility independently.

2. **Unified StdioTransport**: Python maps command types to specialized transport classes (NpxStdioTransport, NodeStdioTransport, etc.). Rust uses a single `StdioTransport` for all commands since the specialized Python transports only differ in how they construct the command line, which Rust handles at the config level.

3. **Elimination of sync-over-async bridge**: Python's `MCPManager` maintains a background event loop thread with `_run_coroutine_threadsafe()` wrappers because the Python call sites are synchronous. Rust's `McpManager` is fully async since the Tokio runtime is available at all call sites. This removes an entire class of threading complexity.

4. **Mixin decomposition to modules**: Python's three mixins (`TransportMixin`, `ConnectionMixin`, `ServerConfigMixin`) are decomposed into separate Rust modules (`transport.rs`, `manager.rs`, `config.rs`) rather than traits. The `McpManager` struct owns all state directly instead of relying on shared mutable `self` attributes.

5. **Typed error enum vs. dict returns**: Python tool calls return `dict` with `success`, `error`, `output` keys. Rust returns `McpResult<McpToolResult>` with typed `McpError` variants, enabling match-based error handling upstream.

6. **Tool namespacing**: Both implementations namespace tools as `mcp__server__tool` (Python) or `server__tool` (Rust) to prevent collisions across servers. The Python handler uses `mcp__` as an additional prefix to distinguish MCP tools from built-in tools at the tool registry level.

7. **Atomic request IDs**: Rust uses `AtomicU64` for thread-safe request ID generation. Python does not explicitly manage request IDs (fastmcp handles it internally).

## Code Examples

### Configuration file format (shared between Python and Rust)

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "transport": "stdio"
    },
    "remote-api": {
      "url": "https://api.example.com/mcp",
      "headers": { "Authorization": "Bearer ${API_TOKEN}" },
      "transport": "http"
    }
  }
}
```

### Rust: Connecting and calling a tool

```rust
let manager = McpManager::new(Some(working_dir));
manager.load_configuration().await?;
manager.connect_all().await?;

// Get all tool schemas for LLM
let schemas = manager.get_all_tool_schemas().await;

// Call a specific tool
let result = manager.call_tool(
    "filesystem",
    "read_file",
    serde_json::json!({"path": "/tmp/test.txt"}),
).await?;

match result.is_error {
    true => eprintln!("Tool error"),
    false => {
        for content in &result.content {
            if let McpContent::Text { text } = content {
                println!("{}", text);
            }
        }
    }
}
```

### Rust: Initialize handshake (internal)

```rust
// Request
JsonRpcRequest {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: Some({
        "protocolVersion": "2024-11-05",
        "capabilities": { "roots": { "listChanged": true } },
        "clientInfo": { "name": "opendev", "version": "0.1.0" }
    }),
}

// Then notification (no response expected)
JsonRpcNotification {
    jsonrpc: "2.0",
    method: "notifications/initialized",
    params: None,
}
```

### Python: Sync tool call with interrupt support

```python
def call_tool_sync(self, server_name, tool_name, arguments, task_monitor=None):
    future = asyncio.run_coroutine_threadsafe(
        self._call_tool_internal(server_name, tool_name, arguments),
        self._event_loop,
    )
    while True:
        if task_monitor and task_monitor.should_interrupt():
            future.cancel()
            return {"success": False, "interrupted": True, ...}
        try:
            return future.result(timeout=0.1)  # 100ms poll
        except concurrent.futures.TimeoutError:
            continue
```

## Remaining Gaps

1. **SSE streaming**: The Rust `SseTransport` currently uses HTTP POST, identical to `HttpTransport`. Full SSE (server-push) streaming is not yet implemented. The Python side delegates to fastmcp's `SSETransport` which handles the stream properly.

2. **Interrupt/cancellation support**: Python's `call_tool_sync` polls a `TaskMonitor` for interrupt signals. The Rust `McpManager::call_tool()` does not yet support cancellation beyond Tokio's standard task cancellation.

3. **Background connection startup**: Python provides `connect_enabled_servers_background()` with an optional completion callback. The Rust equivalent uses `connect_all()` as a standard async method, which the caller can `tokio::spawn` for background execution.

4. **Prompt retrieval**: Python's `MCPManager.get_prompt_sync()` fetches prompt content from servers. The Rust side implements `list_prompts()` but does not yet have a `get_prompt()` method for fetching prompt content.

5. **Config persistence from manager**: Python's `ServerConfigMixin` writes configuration changes to disk via `save_config()`. Rust's `McpManager::add_server()` and `remove_server()` only update in-memory state; disk persistence must be handled by the caller.

6. **Per-server locking**: Python uses `_server_locks` to prevent concurrent connect/disconnect to the same server. Rust relies on the `RwLock` on the connections map, which provides coarser-grained locking.

## References

- Rust source: `crates/opendev-mcp/src/` (config.rs, models.rs, transport.rs, manager.rs, error.rs, lib.rs)
- Python source: `opendev/core/context_engineering/mcp/` (config.py, handler.py, models.py, manager/)
- MCP specification: https://modelcontextprotocol.io
- JSON-RPC 2.0 specification: https://www.jsonrpc.org/specification
