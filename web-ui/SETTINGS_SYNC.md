# Settings Synchronization Between Web UI and Terminal

## Architecture

Both the Web UI and Terminal CLI use **the same configuration files** as the single source of truth:

- `~/.opendev/settings.json` (global settings)
- `.opendev/settings.json` (project-specific settings)

This ensures automatic synchronization without needing special sync logic.

## How It Works

### Web UI Updates Settings:
1. User changes settings in Web UI Settings modal
2. Frontend calls `PUT /api/config` with new values
3. Backend's `ConfigManager.save_config()` writes to disk
4. Terminal CLI automatically reads updated config on next command

### Terminal CLI Updates Settings:
1. User runs commands like `/config model anthropic/claude-3-5-sonnet`
2. `ConfigManager.save_config()` writes to disk
3. Web UI reads updated config via `GET /api/config` on next page load or manual refresh

## Config File Structure

```json
{
  "model_provider": "anthropic",
  "model": "claude-3-5-sonnet-20241022",
  "temperature": 0.7,
  "max_tokens": 4096,
  "enable_bash": true,
  "working_directory": "/Users/user/project"
}
```

## Supported Settings

Currently synced settings:
- **Model Provider** (anthropic, openai, fireworks, etc.)
- **Model** (specific model ID)
- **Temperature** (0.0 - 2.0)
- **Max Tokens** (100 - 32000)

Future settings to add:
- **MCP Servers** configuration
- **Tool permissions** and auto-approval rules
- **UI preferences** (theme, font size)

## Implementation Details

### Backend (`swecli/web/routes/config.py`):
- `GET /api/config` - Returns current configuration
- `PUT /api/config` - Updates and saves configuration
- `GET /api/config/providers` - Lists available AI providers and models

### Frontend (`web-ui/src/components/Settings/`):
- `SettingsModal.tsx` - Main settings dialog with tabs
- `ModelSettings.tsx` - Model and temperature configuration
- `MCPSettings.tsx` - MCP server management (TODO)

### Shared Backend (`swecli/core/management/config_manager.py`):
- `load_config()` - Reads from disk
- `save_config()` - Writes to disk
- Used by both Web UI and Terminal CLI

## Best Practices

1. **Always save after changes**: Both UIs call `save_config()` immediately
2. **Reload when needed**: Web UI should reload config when modal opens
3. **Show current values**: Always display what's actually saved, not cached values
4. **Validate before saving**: Check model availability, valid ranges, etc.

## Future Enhancements

- **Real-time sync**: Use WebSocket to push config changes immediately
- **Conflict resolution**: Handle simultaneous changes from both UIs
- **Change history**: Track who changed what and when
- **Validation**: Prevent invalid configurations from being saved
