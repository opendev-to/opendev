# OpenDev Roadmap

This document outlines OpenDev's development priorities. Features are grouped by maturity and projected timeline. Contributions and feedback are welcome — open an issue to discuss anything here.

---

## ✅ Shipped

- **Compound AI Architecture**: Main agent + 8 specialized subagents (planner, code explorer, PR reviewer, security reviewer, project init, web clone, web generator, ask user)
- **Per-Workflow Model Binding**: Independent model assignment for execution, thinking, critique, compaction, and vision workflows
- **Multi-Provider Support**: Fireworks, Anthropic, OpenAI, Azure, Groq, Mistral, DeepInfra, OpenRouter
- **Context Engineering Layer**: Staged compaction (70%→99%), observation masking, pruning, history archiving
- **ACE Playbook**: Strategy memory with embedding-based retrieval, delta updates, and feedback scoring
- **TUI (Textual)**: Full terminal interface with session management, tool approval, markdown rendering
- **Web UI**: React + Vite frontend with FastAPI/WebSocket backend, real-time streaming, multi-session management
- **25+ Built-in Tools**: File ops, bash, git, edit, browser, web search/fetch/screenshot, VLM, PDF, notebook, memory, session management, scheduling
- **LSP Integration**: 35 language server configurations (Python, TypeScript, Go, Rust, Java, C#, Ruby, and more)
- **MCP Integration**: Dynamic tool discovery via Model Context Protocol
- **Docker Runtime**: Local and remote execution with sandboxed containers
- **Plugin System**: Hook-based extensibility with lifecycle management
- **Plan Mode**: 5-phase planning workflow with explore agents
- **Session Persistence**: Save, resume, and manage conversation sessions
- **Concurrent Sessions**: Multiple independent agent sessions running in parallel
- **Channel Adapter Framework**: Unified messaging abstraction with `InboundMessage`/`OutboundMessage` models and a message router, ready for multi-channel integrations

---

## 🚧 In Progress

- **Remote Web UI Sessions**: Deploy the Web UI as a remote server, enabling mobile access and async task execution
- **Proactive Agent Loop**: Background execution where the agent continues working autonomously between user prompts
- **Telegram Integration**: Chat with your OpenDev agent via Telegram bot (adapter skeleton in place)
- **WhatsApp Integration**: Interact with OpenDev via WhatsApp Business API (adapter skeleton in place)
- **Enhanced Docker Sandboxing**: Improved isolation for untrusted code execution

---

## 🗺️ Planned

### Short-Term
- **GitHub / GitLab Integration**: Trigger tasks from issues and PRs, auto-create pull requests from completed work
- **Test-Driven Workflow**: Automated test generation, red-green-refactor loops, coverage tracking
- **CI/CD Awareness**: Read pipeline status, fix failing builds, suggest deployment steps

### Medium-Term
- **Slack / Discord Integration**: Bring OpenDev into team channels, trigger tasks and receive updates
- **Email Channel**: Send coding tasks via email, receive diffs and summaries back
- **iMessage / Signal Integration**: Personal messaging channels for mobile-first async workflows
- **Multi-Repo Support**: Work across multiple repositories in a single session
- **Team Collaboration**: Shared sessions and handoff between agents and developers
- **Custom Agent Definitions**: User-defined subagents with configurable prompts, tools, and model bindings
- **Persistent Knowledge Base**: Cross-session memory for project context, patterns, and decisions

### Long-Term
- **Full SDLC Orchestration**: End-to-end support from requirements → design → implementation → testing → review → deployment
- **Self-Improving Playbook**: Automatic strategy refinement based on execution outcomes across sessions
- **Agent Marketplace**: Community-contributed subagents, skills, and workflows

---

## 💡 Community Ideas

Have a feature request? [Open an issue](https://github.com/opendev-to/opendev/issues) and let's discuss.
