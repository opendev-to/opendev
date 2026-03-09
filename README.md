<p align="center">
  <img src="logo/logo_long.png" alt="OpenDev Logo" width="400"/>
</p>

<p align="center">The open source AI coding agent for your terminal.</p>

<p align="center">
  <a href="https://pypi.org/project/opendev/"><img alt="PyPI version" src="https://img.shields.io/pypi/v/opendev?style=flat-square" /></a>
  <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square" /></a>
  <a href="https://python.org/"><img alt="Python version" src="https://img.shields.io/badge/python-%3E%3D3.10-blue.svg?style=flat-square" /></a>
  <a href="https://arxiv.org/pdf/2603.05344"><img alt="Technical Report" src="https://img.shields.io/badge/Technical%20Report-arXiv-b31b1b.svg?style=flat-square" /></a>
</p>

<p align="center">
  🌐 <b>Website & Docs are coming soon — stay tuned!</b> 🚀
</p>

<p align="center">
  <img src="figures/introduction.png" alt="OpenDev Introduction" width="800"/>
</p>

---

### Introduction

OpenDev is an open-source, terminal-native coding agent built as a compound AI system. Instead of a single monolithic LLM, it uses a structured ensemble of agents and workflows — each independently bound to a user-configured model.

Work is organized into concurrent sessions composed of specialized sub-agents. Each agent executes typed workflows (Execution, Thinking, Compaction) that independently bind to an LLM, enabling fine-grained cost, latency, and capability trade-offs per workflow.

Each workflow is a modular slot you can bind to any LLM of your choice: **Normal** (execution), **Thinking** (reasoning), **Compact** (context summarization), **Self-Critique** (output verification), and **VLM** (vision). For example, use Claude Opus for execution, GPT-o3 for thinking, and a lightweight Qwen model for compaction. This lets advanced users fine-tune cost, speed, and quality at per-workflow granularity.

<p align="center">
  <img src="figures/top.png" alt="OpenDev Compound AI Architecture" width="700"/>
</p>


---

### Why OpenDev?

We designed OpenDev with a bold vision: **what if your coding agent didn't stop working just because you did?**

#### 🤖 Proactive Coding Agent

OpenDev isn't just a reactive assistant that waits for your next prompt. It is designed to be a **proactive coding agent** that can plan, execute, and iterate on tasks — whether you're actively pairing with it or fast asleep. Kick off a complex refactoring, go grab dinner, and come back to a pull request waiting for your review.

#### 🔀 True Multi-Provider, Multi-Model Flexibility

Most AI coding tools lock you into a single provider. OpenDev breaks that wall down. You can assign a **different model from a different provider to every session**, and they all run **in parallel**. Want Claude for planning, GPT for execution, and Gemini for code review — all in the same project, at the same time? Done. Your models, your rules.

#### 💻 TUI + Web UI — Code From Anywhere

OpenDev ships with both a **terminal UI (TUI)** for the terminal purists and a **Web UI** for when you want something a little more visual. Here's where it gets fun: the Web UI can be configured as a **remote session**.

That means you can fire up a coding task from your phone, tuck yourself into bed, and let OpenDev keep shipping code while you drift off to dreamland. 🛏️📱💤

> *"I deployed to production from under my blanket at 2 AM and woke up to passing tests. Is this what peak engineering looks like?"*
> — You, probably, after using OpenDev

---

### Installation

```bash
# With uv (recommended)
uv pip install opendev

# With pip
pip install opendev
```

### Quick Start

```bash
# Configure your LLM providers
opendev config setup

# Start the interactive TUI
opendev

# Or start the Web UI
opendev run ui

# Single prompt (non-interactive)
opendev -p "explain this codebase"

# Resume most recent session
opendev --continue
```

<p align="center">
  <img src="figures/web_ui.png" alt="OpenDev Web UI" width="800"/>
</p>

### Multi-Provider Support

OpenDev is not coupled to any single provider. It supports OpenAI, Anthropic, Fireworks, Google, and any OpenAI-compatible endpoint. Different tasks (planning, execution, compaction) can each bind to a different model, letting you optimize cost and capability independently.

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
uv venv && uv pip install -e ".[dev]"
source .venv/bin/activate

# Run tests
uv run pytest

# Code quality
black opendev/ tests/ --line-length 100
ruff check opendev/ tests/ --fix
mypy opendev/

# Build the Web UI frontend
cd web-ui && npm run build
```

### Contributing

If you're interested in contributing to OpenDev, please open an issue or submit a pull request.

---

### How OpenDev Compares

- **vs. Claude Code / Codex CLI / Gemini CLI:** Closed-source tools that lock you into a single provider. OpenDev is fully open source and lets you mix models from any provider, independently bound per workflow (execution, thinking, critique, compaction, vision).
- **vs. OpenCode:** OpenCode is a great open-source coding agent with TUI, Web UI, and LSP support. However, its architecture is not modular enough to support per-workflow model binding, concurrent multi-agent sessions, or compound AI orchestration.
- **vs. OpenClaw:** OpenDev and OpenClaw share similar concepts around autonomous AI agents. The key difference is focus: OpenDev is purpose-built for the software development lifecycle, with context engineering, structured agent workflows, and deep code understanding.

📋 See the [Roadmap](./ROADMAP.md) for what's shipped, in progress, and planned.

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
