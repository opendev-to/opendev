# Elixir Language Server Integration

This directory contains the integration for Elixir language support using [Next LS](https://github.com/elixir-tools/next-ls) from the elixir-tools project.

> **⚠️ Windows Not Supported**: Next LS does not provide Windows binaries, so Elixir language server integration is only available on Linux and macOS.

## Known Issues

### Next LS v0.23.3 Timeout Enumeration Bug
There is a known intermittent bug in Next LS v0.23.3 where `textDocument/definition` requests can fail with:
```
Protocol.UndefinedError: protocol Enumerable not implemented for :timeout of type Atom
```

This bug is tracked in [Next LS Issue #543](https://github.com/elixir-tools/next-ls/issues/543) and primarily occurs in CI environments. The affected test (`test_request_defining_symbol_none`) is marked as expected to fail until this upstream bug is resolved.

## Prerequisites

Before using the Elixir language server integration, you need to have:

1. **Elixir** installed and available in your PATH
   - Install from: https://elixir-lang.org/install.html
   - Verify with: `elixir --version`

2. **Next LS** installed and available in your PATH
   - Install from: https://github.com/elixir-tools/next-ls#installation
   - Verify with: `nextls --version`

## Features

The Elixir integration provides:

- **Language Server Protocol (LSP) support** via Next LS
- **File extension recognition** for `.ex` and `.exs` files
- **Project structure awareness** with proper handling of Elixir-specific directories:
  - `_build/` - Compiled artifacts (ignored)
  - `deps/` - Dependencies (ignored)
  - `.elixir_ls/` - ElixirLS artifacts (ignored)
  - `cover/` - Coverage reports (ignored)
  - `lib/` - Source code (not ignored)
  - `test/` - Test files (not ignored)

## Configuration

The integration uses the default Next LS configuration with:

- **MIX_ENV**: `dev`
- **MIX_TARGET**: `host`
- **Experimental completions**: Disabled by default
- **Credo extension**: Enabled by default

## Usage

The Elixir language server is automatically selected when working with Elixir projects. It will be used for:

- Code completion
- Go to definition
- Find references
- Document symbols
- Hover information
- Code formatting
- Diagnostics (via Credo integration)

### Important: Project Compilation

Next LS requires your Elixir project to be **compiled** for optimal performance, especially for:
- Cross-file reference resolution
- Complete symbol information
- Accurate go-to-definition

**For production use**: Ensure your project is compiled with `mix compile` before using the language server.

**For testing**: The test suite automatically compiles the test repositories before running tests to ensure optimal Next LS performance.

## Testing

Run the Elixir-specific tests with:

```bash
pytest test/solidlsp/elixir/ -m elixir
```

## Implementation Details

- **Main class**: `ElixirTools` in `elixir_tools.py`
- **Initialization parameters**: Defined in `initialize_params.json`
- **Language identifier**: `"elixir"`
- **Command**: `nextls --stdio`

The implementation follows the same patterns as other language servers in this project, inheriting from `SolidLanguageServer` and providing Elixir-specific configuration and behavior. 