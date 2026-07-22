# Provider Setup Guide

OpenDev supports 9 LLM providers out of the box. This guide covers authentication, provider configuration, and workflow model binding.

## Getting Started

The fastest way to start is to export an API key and run OpenDev:

```bash
export OPENAI_API_KEY="sk-..."
opendev
```

OpenDev will auto-detect the key and use OpenAI as your provider. You can swap to any supported provider by exporting the corresponding key instead:

```bash
# Anthropic
export ANTHROPIC_API_KEY="sk-ant-..."

# Fireworks
export FIREWORKS_API_KEY="fw_..."
```

Alternatively, run the interactive setup wizard to configure providers, models, and workflow bindings in one step:

```bash
opendev config setup
```

## Authentication Precedence

OpenDev resolves API keys in this order:

- **Environment variable** (highest priority) -- e.g. `OPENAI_API_KEY`
- **Stored credential** in `~/.opendev/auth.json` -- written by `opendev config setup` with `0600` permissions
- **Interactive prompt** -- if no key is found, OpenDev prompts you during setup

Environment variables always win. This lets you override stored credentials per-shell or in CI without touching config files.

## Supported Providers

### Experimental ChatGPT/Codex OAuth

`openai-chatgpt` is an opt-in, experimental ChatGPT/Codex OAuth provider. It
is not an OpenAI Platform API-key provider, cannot be pointed at a custom
`api_base_url`, and never falls back to `OPENAI_API_KEY`.

**Release status: compatibility approval pending.** OpenAI's public guidance
documents ChatGPT OAuth and device-code login for the Codex CLI, not for a
separate third-party provider. Do not enable or distribute this integration
until an OpenDev maintainer has recorded Phase 0 approval for the standalone
OAuth profile, supported account/workspace types, and protocol ownership. The
implementation and mocked tests exist to support that review; they are not an
authorization to use a ChatGPT subscription as a general API replacement.

After that approval, use the first-run setup flow: select **OpenAI**, then
choose **ChatGPT Pro/Plus (browser)** or **ChatGPT Pro/Plus (headless)**. The
wizard always asks you to sign in and, after a successful sign-in, replaces
the previous local OAuth credential. It then loads OpenAI model
metadata from OpenDev's dynamically refreshed models.dev catalogue (the same
catalogue approach used by OpenCode), and writes `experimental.chatgpt_auth = true`,
`model_provider = "openai-chatgpt"`, and the selected model to
`~/.opendev/settings.json`. The model list is discovery metadata, not an entitlement
guarantee; an unavailable model is reported by the backend rather than falling
back to Platform API-key authentication.

Browser login uses a loopback PKCE callback. `--headless` uses the device-code
flow, shows a verification URL and code, then polls until it can exchange the
returned authorization code with its PKCE verifier. Device login can require a
personal security setting or workspace-admin permission. Credentials are stored
only in OpenDev's local auth store; the flow does not require `codex` or read
Codex's credential store. To sign in again, change accounts, or recover from a
missing/expired credential, rerun `opendev setup` and select the ChatGPT option.

The initial credential backend is the plaintext `~/.opendev/auth.json` file
with owner-only permissions on Unix. Do not run multiple OpenDev processes
that refresh the same ChatGPT credential at once: refresh-token rotation is
single-flight within a process, and OpenDev warns about the cross-process
limitation.

### OpenAI

- Env var: `OPENAI_API_KEY`
- Provider ID: `openai`
- Popular models: `gpt-4o`, `gpt-4o-mini`, `o3`, `o4-mini`

```bash
export OPENAI_API_KEY="sk-..."
```

### Anthropic

- Env var: `ANTHROPIC_API_KEY`
- Provider ID: `anthropic`
- Popular models: `claude-sonnet-4-20250514`, `claude-opus-4-20250514`

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### Fireworks

- Env var: `FIREWORKS_API_KEY`
- Provider ID: `fireworks`
- Popular models: `accounts/fireworks/models/kimi-k2-instruct-0905`
- Note: Fireworks model IDs use the `accounts/fireworks/models/` prefix. OpenDev auto-normalizes short names (e.g. `kimi-k2-instruct-0905` becomes the full path).

```bash
export FIREWORKS_API_KEY="fw_..."
```

### Other Providers

All of these follow the same pattern -- export the env var and set the provider ID in your config:

- **Google** -- `GOOGLE_API_KEY`, provider ID `google`
- **Groq** -- `GROQ_API_KEY`, provider ID `groq`
- **Mistral** -- `MISTRAL_API_KEY`, provider ID `mistral`
- **DeepInfra** -- `DEEPINFRA_API_KEY`, provider ID `deepinfra`
- **OpenRouter** -- `OPENROUTER_API_KEY`, provider ID `openrouter`
- **Azure OpenAI** -- `AZURE_OPENAI_API_KEY`, provider ID `azure`

## Custom OpenAI-Compatible Endpoints

OpenDev also supports custom OpenAI-compatible `chat/completions` endpoints through `api_base_url`.

Use a custom provider name and point `api_base_url` at the provider's base compatibility URL. OpenDev will append `/chat/completions` automatically unless the URL already ends with it.

Today, custom providers use `OPENAI_API_KEY` as the env-var fallback. That means you can map another provider token into `OPENAI_API_KEY` for the current shell.

### Example: Cloudflare AI Gateway

This configuration was validated against Cloudflare's OpenAI-compatible compatibility endpoint:

```json
{
  "model_provider": "cloudflare",
  "model": "openai/gpt-4o-mini",
  "api_base_url": "https://gateway.ai.cloudflare.com/v1/def31e2cf1530789c604bdaa2abbfcf1/openai-proxy/compat"
}
```

Run it with:

```bash
export OPENAI_API_KEY="$CF_AIG_TOKEN"
opendev -p "What is Cloudflare? Reply in one sentence."
```

Effective request URL:

```text
https://gateway.ai.cloudflare.com/v1/def31e2cf1530789c604bdaa2abbfcf1/openai-proxy/compat/chat/completions
```

### Notes

- Use a custom `model_provider` value such as `cloudflare` so OpenDev takes the generic OpenAI-compatible path.
- For custom providers, `api_base_url` should be the compatibility base URL, not the full `/chat/completions` path unless you want to set it explicitly.
- Environment variables now override stored `api_key` values from config, which makes shell-scoped testing of custom endpoints work correctly.

## Offline and Air-Gapped Use

OpenDev fetches its provider/model catalog from [models.dev](https://models.dev/api.json) at startup and caches it under `~/.opendev/cache/providers`. The catalog is optional: when models.dev is unreachable, OpenDev falls back to built-in defaults for 12 well-known providers (OpenAI, Anthropic, Google, Groq, Fireworks, Mistral, DeepSeek, OpenRouter, Together, xAI, Ollama, LM Studio), so fully offline setups keep working — including the first-run setup wizard.

Environment variables for offline environments:

- `OPENDEV_DISABLE_REMOTE_MODELS` — set to `1` (or `true`/`yes`) to skip the models.dev fetch entirely. Recommended on air-gapped machines to avoid the network timeout at startup.
- `OPENDEV_MODELS_DEV_PATH` — path to a local copy of the models.dev catalog (`api.json`). Lets you supply a full model catalog without network access.
- `OPENDEV_API_BASE_URL` — overrides the API base URL for the active provider (equivalent to `api_base_url` in `settings.json`). Point it at your local OpenAI-compatible endpoint (llama-server, Ollama, LM Studio, vLLM, ...). Include the `http://` scheme.

Example: fully offline against a local llama-server:

```bash
export OPENDEV_DISABLE_REMOTE_MODELS=1
export OPENDEV_API_BASE_URL="http://127.0.0.1:8080/v1"
opendev -p "hello"
```

The local providers `ollama` and `lmstudio` require no API key; their default endpoints (`http://localhost:11434` and `http://localhost:1234`) are built in.

## Workflow Model Binding

OpenDev is a compound AI system. Instead of one model doing everything, it has workflow slots, each independently bound to a model and provider:

- **Normal** (`model` + `model_provider`) -- The primary execution model. Handles coding tasks, tool calls, and general conversation. This is the only required slot.
- **Thinking** (`model_thinking` + `model_thinking_provider`) -- Used for complex reasoning in plan mode and deep analysis. Falls back to Normal if not set.
- **Compact** (`agents.compact`) -- Summarizes conversation history when context gets too long. Falls back to Normal if not set. Configured via the `agents` map (see below).
- **VLM** (`model_vlm` + `model_vlm_provider`) -- Processes images and screenshots. Falls back to Normal if the Normal model has vision capability; otherwise unavailable.

### Fallback Chains

You only need to configure the slots you want to customize. Unset slots fall back automatically:

- Thinking -> Normal
- Compact -> Normal
- VLM -> Normal (only if Normal model supports vision)

### Example: Mixed-Provider Configuration

Use Claude for execution, OpenAI o3 for thinking, and a fast Fireworks model for compaction:

```json
{
  "model_provider": "anthropic",
  "model": "claude-sonnet-4-20250514",

  "model_thinking_provider": "openai",
  "model_thinking": "o3",

  "agents": {
    "compact": {
      "model": "accounts/fireworks/models/kimi-k2-instruct-0905",
      "provider": "fireworks"
    }
  }
}
```

This requires `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, and `FIREWORKS_API_KEY` to be set.

## Configuration Files

OpenDev uses a hierarchical config system. Settings are JSON files with optional `//` and `/* */` comments.

### Config Hierarchy (highest priority first)

- **Project config**: `.opendev/settings.json` in your project root
- **Global config**: `~/.opendev/settings.json`
- **Defaults**: Built-in default values

Project config overrides global config, which overrides defaults. The exception is `instructions` -- instructions from all levels are concatenated, not overridden.

### Variable Substitution

Config values support two substitution patterns:

- `{env:VAR_NAME}` -- replaced with the value of environment variable `VAR_NAME`
- `{file:/path/to/file}` -- replaced with the contents of the file

Example:

```json
{
  "model_provider": "openai",
  "model": "gpt-4o",
  "instructions": "Project context: {file:./CONTEXT.md}"
}
```

### Credential Storage

API keys are stored separately from config in `~/.opendev/auth.json` (mode `0600`). Keys set via `opendev config setup` go here. Environment variables always take precedence over stored keys.

Never put API keys in `settings.json` -- the config system intentionally strips `api_key` fields from config files for security.
