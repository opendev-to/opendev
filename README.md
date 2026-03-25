<p align="center">
  <img src="logo/logo_long.png" alt="OpenDev Logo" width="400"/>
</p>

<p align="center">Open-source AI coding agent that spawns parallel agents, each bound to the LLM of your choice.</p>

<p align="center">
  <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square" /></a>
  <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/rust-%3E%3D1.94-orange.svg?style=flat-square" /></a>
  <a href="https://arxiv.org/pdf/2603.05344"><img alt="Technical Report" src="https://img.shields.io/badge/Technical%20Report-arXiv-b31b1b.svg?style=flat-square" /></a>
</p>

<p align="center">
  <strong>Website and documentation coming soon!</strong>
</p>

<p align="center">
  <img src="demo_assets/demo.gif" alt="OpenDev Demo" width="800"/>
</p>

---

### Introduction

OpenDev is an open-source, terminal-native coding agent built as a compound AI system. Instead of a single monolithic LLM, it uses a structured ensemble of agents and workflows -- each independently bound to a user-configured model.

Work is organized into concurrent sessions composed of specialized sub-agents. Each agent executes typed workflows (Execution, Thinking, Compaction) that independently bind to an LLM, enabling fine-grained cost, latency, and capability trade-offs per workflow.

Each workflow is a modular slot you can bind to any LLM of your choice: **Normal** (execution), **Thinking** (reasoning), **Compact** (context summarization), **Self-Critique** (output verification), and **VLM** (vision). For example, use Claude Opus for execution, GPT-o3 for thinking, and a lightweight Qwen model for compaction. Together, these combinations form a compound AI system where multiple models collaborate, each optimized for its role.

OpenDev is written in **Rust** for maximum performance and minimal resource usage. Looking for the original Python version? See [opendev-py](https://github.com/opendev-to/opendev-py) (archived).

<p align="center">
  <img src="figures/top.png" alt="OpenDev Compound AI Architecture" width="700"/>
</p>

---

### Why OpenDev?

- **Proactive, not reactive.** OpenDev can plan, execute, and iterate autonomously. Kick off a refactoring, walk away, and come back to a PR ready for review.
- **Multi-provider, multi-model.** Assign different models from different providers to every workflow and session, all running in parallel. Your models, your rules.
- **TUI + Web UI.** A full terminal UI for power users and a Web UI for visual monitoring. The Web UI supports remote sessions, so you can start a task from your phone and let OpenDev work while you sleep.
- **Fast.** Written in Rust with a ~3.7MB release binary. Instant startup, low memory footprint.

---

### Installation

#### macOS

```bash
# Homebrew (recommended)
brew install opendev-to/tap/opendev

# Shell installer
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/opendev-to/opendev/releases/latest/download/opendev-cli-installer.sh | sh

# Or download the binary directly from GitHub Releases:
#   opendev-cli-aarch64-apple-darwin.tar.xz  (Apple Silicon)
#   opendev-cli-x86_64-apple-darwin.tar.xz   (Intel)
```

#### Linux

```bash
# Shell installer (x86_64 and ARM64)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/opendev-to/opendev/releases/latest/download/opendev-cli-installer.sh | sh

# Or download the binary directly from GitHub Releases:
#   opendev-cli-x86_64-unknown-linux-gnu.tar.xz   (x86_64)
#   opendev-cli-aarch64-unknown-linux-gnu.tar.xz   (ARM64 / Raspberry Pi)
```

#### Windows

```powershell
# PowerShell installer
powershell -ExecutionPolicy ByPass -c "irm https://github.com/opendev-to/opendev/releases/latest/download/opendev-cli-installer.ps1 | iex"

# Or download opendev-cli-x86_64-pc-windows-msvc.zip from GitHub Releases
```

#### From source (all platforms)

Requires [Rust](https://rustup.rs/) 1.94+.

```bash
git clone https://github.com/opendev-to/opendev.git
cd opendev
cargo build --release -p opendev-cli
# Binary at target/release/opendev (or opendev.exe on Windows)
```

If you use the repo for development, you may also have a local symlink at `~/.local/bin/opendev` pointing at `target/release/opendev`. That can take precedence over the Homebrew binary in `/opt/homebrew/bin/opendev`.

To test a Homebrew install from a clean shell state:

```bash
rm -f ~/.local/bin/opendev
hash -r
brew uninstall opendev
brew untap opendev-to/tap
brew tap opendev-to/tap
brew install opendev-to/tap/opendev
which opendev
opendev --version
```

See [DEVELOPMENT.md](./DEVELOPMENT.md) for the full local development and Homebrew testing workflow.

> **All release binaries, checksums, and installers are available on the [GitHub Releases](https://github.com/opendev-to/opendev/releases) page.**

#### Supported platforms

| Platform | Architecture | Binary |
|----------|-------------|--------|
| macOS | Apple Silicon (M1+) | `opendev-cli-aarch64-apple-darwin.tar.xz` |
| macOS | Intel | `opendev-cli-x86_64-apple-darwin.tar.xz` |
| Linux | x86_64 | `opendev-cli-x86_64-unknown-linux-gnu.tar.xz` |
| Linux | ARM64 | `opendev-cli-aarch64-unknown-linux-gnu.tar.xz` |
| Windows | x86_64 | `opendev-cli-x86_64-pc-windows-msvc.zip` |

#### Verify installation

```bash
opendev --version
```

If Homebrew reports `Not a valid ref: refs/remotes/origin/main` while auto-updating the tap, remove the stale local tap clone and retry:

```bash
brew untap opendev-to/tap
brew tap opendev-to/tap
brew install opendev-to/tap/opendev
```

### Quick Start

```bash
# Set an API key (OpenAI, Anthropic, or Fireworks -- any one will do)
export OPENAI_API_KEY="sk-..."
# export ANTHROPIC_API_KEY="sk-ant-..."
# export FIREWORKS_API_KEY="fw_..."

# Start the interactive TUI
opendev

# Or start the Web UI
opendev run ui

# Single prompt (non-interactive)
opendev -p "explain this codebase"

# Resume most recent session
opendev --continue
```

Prefer a guided walkthrough? Run `opendev config setup` to interactively choose providers, models, and workflow bindings.

See the [Provider Setup Guide](docs/providers.md) for all 9 supported providers, authentication details, and advanced configuration.

<p align="center">
  <img src="figures/web_ui.png" alt="OpenDev Web UI" width="800"/>
</p>

### Multi-Provider Support

OpenDev supports 9 LLM providers: **OpenAI**, **Anthropic**, **Fireworks**, **Google**, **Groq**, **Mistral**, **DeepInfra**, **OpenRouter**, and **Azure OpenAI**.

Each provider's models can be independently assigned to 5 workflow slots:

- **Normal** -- Primary execution model for coding tasks and tool calls
- **Thinking** -- Complex reasoning and planning (falls back to Normal)
- **Compact** -- Context summarization when history grows long (falls back to Normal)
- **Critique** -- Self-critique of agent reasoning (falls back to Thinking)
- **VLM** -- Vision/image processing (falls back to Normal if it supports vision)

Mix and match providers per slot in `~/.opendev/settings.json`:

```json
{
  "model_provider": "anthropic",
  "model": "claude-sonnet-4-20250514",
  "model_thinking_provider": "openai",
  "model_thinking": "o3"
}
```

See the [Provider Setup Guide](docs/providers.md) for the full list of env vars, fallback chains, and configuration options.

### MCP Integration

Dynamic tool discovery via the Model Context Protocol for connecting to external tools and data sources.

```bash
opendev mcp list
opendev mcp add myserver uvx mcp-server-sqlite
opendev mcp enable/disable myserver
```

### Development

```bash
git clone https://github.com/opendev-to/opendev.git
cd opendev
cargo build --workspace
cargo test --workspace
```

```bash
cargo check --workspace       # Type check
cargo clippy --workspace      # Lint
cargo fmt --all               # Format
cargo test -p opendev-cli     # Test a specific crate
```

Detailed local-dev, symlink, Homebrew, and release-testing notes are in [DEVELOPMENT.md](./DEVELOPMENT.md).

### Web UI

The frontend is a React/Vite app in `web-ui/`:

```bash
cd web-ui && npm ci && npm run build
```

### Contributing

If you're interested in contributing to OpenDev, please open an issue or submit a pull request.

---

### How OpenDev Compares

- **vs. Claude Code / Codex CLI / Gemini CLI:** Closed-source tools that lock you into a single provider. OpenDev is fully open source and lets you mix models from any provider, independently bound per workflow (execution, thinking, critique, compaction, vision).
- **vs. OpenCode:** OpenCode is a great open-source coding agent with TUI, Web UI, and LSP support. However, its architecture is not modular enough to support per-workflow model binding, concurrent multi-agent sessions, or compound AI orchestration.
- **vs. OpenClaw:** OpenDev and OpenClaw share similar concepts around autonomous AI agents. The key difference is focus: OpenDev is purpose-built for the software development lifecycle, with context engineering, structured agent workflows, and deep code understanding.

---

### Star History

<p align="center">
  <a href="https://star-history.com/#opendev-to/opendev&Date">
   <picture>
     <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=opendev-to/opendev&type=Date&theme=dark" />
     <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=opendev-to/opendev&type=Date" />
     <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=opendev-to/opendev&type=Date" />
   </picture>
  </a>
</p>
