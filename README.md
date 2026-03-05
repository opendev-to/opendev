<p align="center">
  <img src="logo/swe-cli-high-resolution-logo-grayscale-transparent.png" alt="SWE-CLI Logo" width="400"/>
</p>

<p align="center">
  <strong>One-Stop, Cost-Effective CLI-based Coding Agent for Modern Software Engineering</strong>
</p>

<p align="center">
  Interactive TUI • MCP Integration • Multi-Provider Support • SOLID Architecture
</p>

<p align="center">
  <a href="https://pypi.org/project/swe-cli/"><img alt="PyPI version" src="https://img.shields.io/pypi/v/swe-cli?style=flat-square" /></a>
  <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square" /></a>
  <a href="https://python.org/"><img alt="Python version" src="https://img.shields.io/badge/python-%3E%3D3.10-blue.svg?style=flat-square" /></a>
  <a href="https://github.com/swe-cli/swe-cli/issues"><img alt="Issues" src="https://img.shields.io/github/issues/swe-cli/swe-cli?style=flat-square" /></a>
  <a href="https://github.com/swe-cli/swe-cli/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/swe-cli/swe-cli?style=flat-square" /></a>
</p>

<p align="center">
  <a href="#overview"><strong>Overview</strong></a> •
  <a href="#installation"><strong>Installation</strong></a> •
  <a href="#quick-start"><strong>Quick Start</strong></a> •
  <a href="#key-components"><strong>Key Components</strong></a>
</p>

## Overview

**SWE-CLI** is a one-stop, cost-effective CLI-based coding agent designed to democratize how coding agents are built. It supports **MCP (Model Context Protocol)**, **multi-provider LLMs** (Fireworks, OpenAI, Anthropic), and deep **codebase understanding** through a modular, SOLID-based architecture.

## Installation

We recommend using [uv](https://github.com/astral-sh/uv) for fast and reliable installation.

### User Installation
```bash
uv pip install swe-cli
```

### Development Setup

#### 1. Clone and install dependencies

```bash
git clone https://github.com/swe-cli/swe-cli.git
cd swe-cli

# Create venv and install the package with dev dependencies
uv venv
uv pip install -e ".[dev]"
```

#### 2. Activate the virtual environment

```bash
source .venv/bin/activate
```

#### 3. Run the app

```bash
# After activating venv
swecli

# Or without activating
uv run swecli
```

#### 4. Run pytest

```bash
# Run all tests
uv run pytest

# Run specific test file
uv run pytest tests/test_terminal_box_renderer.py

# Run with verbose output
uv run pytest -v

# Run with coverage
uv run pytest --cov=swecli
```

#### Quick one-liner setup

```bash
uv venv && uv pip install -e ".[dev]" && uv run pytest
```

## Quick Start

1.  **Configure**: Run the setup wizard to configure your LLM providers.
    ```bash
    swecli config setup
    ```

2.  **Run**: Start the interactive coding assistant.
    ```bash
    swecli
    ```
    *Or start the Web UI:*
    ```bash
    swecli run ui
    ```

## Key Components

*   **Interactive TUI**: A full-screen, Textual-based terminal interface for seamless interaction.
*   **MCP Support**: Extensible architecture using the Model Context Protocol to connect with external tools and data.
*   **Multi-Provider**: Native support for Fireworks, OpenAI, and Anthropic models.
*   **Session Management**: Persistent conversation history and context management.
*   **SOLID Architecture**: Built with clean, maintainable code using dependency injection and interface-driven design.

## License

[MIT](LICENSE)
