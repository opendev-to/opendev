"""MCP configuration management."""

import json
import os
from pathlib import Path
from typing import Optional

from swecli.core.context_engineering.mcp.models import MCPConfig, MCPServerConfig
from swecli.core.paths import get_paths


def get_config_path() -> Path:
    """Get the path to the MCP configuration file."""
    paths = get_paths()
    paths.global_dir.mkdir(parents=True, exist_ok=True)
    return paths.global_mcp_config


def get_project_config_path(working_dir: Optional[Path] = None) -> Optional[Path]:
    """Get the path to the project-level MCP configuration file.

    Args:
        working_dir: Working directory. If None, uses current directory.

    Returns:
        Path to .mcp.json if it exists, None otherwise
    """
    paths = get_paths(working_dir)
    project_config = paths.project_mcp_config
    return project_config if project_config.exists() else None


def load_config(config_path: Optional[Path] = None) -> MCPConfig:
    """Load MCP configuration from file.

    Args:
        config_path: Path to config file. If None, uses default global config.

    Returns:
        MCPConfig object with loaded configuration
    """
    if config_path is None:
        config_path = get_config_path()

    if not config_path.exists():
        # Return empty config
        return MCPConfig(mcpServers={})

    try:
        with open(config_path, "r") as f:
            data = json.load(f)
        return MCPConfig(**data)
    except Exception as e:
        print(f"Warning: Failed to load MCP config from {config_path}: {e}")
        return MCPConfig(mcpServers={})


def save_config(config: MCPConfig, config_path: Optional[Path] = None) -> None:
    """Save MCP configuration to file.

    Args:
        config: MCPConfig object to save
        config_path: Path to config file. If None, uses default global config.
    """
    if config_path is None:
        config_path = get_config_path()

    # Ensure parent directory exists
    config_path.parent.mkdir(parents=True, exist_ok=True)

    # Convert to dict using alias (mcpServers instead of mcp_servers)
    data = config.model_dump(by_alias=True)

    with open(config_path, "w") as f:
        json.dump(data, f, indent=2)


def merge_configs(global_config: MCPConfig, project_config: Optional[MCPConfig]) -> MCPConfig:
    """Merge global and project-level MCP configurations.

    Project config takes precedence over global config.

    Args:
        global_config: Global MCP configuration
        project_config: Project-level MCP configuration (can be None)

    Returns:
        Merged MCPConfig
    """
    if project_config is None:
        return global_config

    # Start with global servers
    merged_servers = dict(global_config.mcp_servers)

    # Override with project servers
    merged_servers.update(project_config.mcp_servers)

    return MCPConfig(mcpServers=merged_servers)


def expand_env_vars(value: str) -> str:
    """Expand environment variables in a string.

    Supports ${VAR_NAME} syntax.

    Args:
        value: String that may contain ${VAR_NAME} patterns

    Returns:
        String with environment variables expanded
    """
    import re

    def replace_var(match):
        var_name = match.group(1)
        return os.environ.get(var_name, match.group(0))  # Return original if not found

    return re.sub(r'\$\{([^}]+)\}', replace_var, value)


def prepare_server_config(server_config: MCPServerConfig) -> MCPServerConfig:
    """Prepare server config by expanding environment variables.

    Args:
        server_config: Server configuration

    Returns:
        Server configuration with expanded environment variables
    """
    # Expand env vars in args
    expanded_args = [expand_env_vars(arg) for arg in server_config.args]

    # Expand env vars in environment variables
    expanded_env = {key: expand_env_vars(value) for key, value in server_config.env.items()}

    # Expand env vars in URL (for HTTP/SSE transport)
    expanded_url = expand_env_vars(server_config.url) if server_config.url else None

    # Expand env vars in headers (for HTTP/SSE transport)
    expanded_headers = {key: expand_env_vars(value) for key, value in server_config.headers.items()}

    return MCPServerConfig(
        command=server_config.command,
        args=expanded_args,
        env=expanded_env,
        url=expanded_url,
        headers=expanded_headers,
        enabled=server_config.enabled,
        auto_start=server_config.auto_start,
        transport=server_config.transport,
    )


def save_server_config(
    name: str,
    config: MCPServerConfig,
    project_config: bool = False,
    working_dir: Optional[Path] = None
) -> None:
    """Save or update a single server configuration.

    Args:
        name: Server name
        config: Server configuration
        project_config: If True, save to project config, else global
        working_dir: Working directory for project config
    """
    paths = get_paths(working_dir)
    if project_config:
        config_path = paths.project_mcp_config
    else:
        config_path = get_config_path()

    # Load existing config
    existing_config = load_config(config_path)

    # Update server
    existing_config.mcp_servers[name] = config

    # Save config
    save_config(existing_config, config_path)


def remove_server_config(
    name: str,
    project_config: bool = False,
    working_dir: Optional[Path] = None
) -> None:
    """Remove a server configuration.

    Args:
        name: Server name to remove
        project_config: If True, remove from project config, else global
        working_dir: Working directory for project config
    """
    paths = get_paths(working_dir)
    if project_config:
        config_path = paths.project_mcp_config
    else:
        config_path = get_config_path()

    # Load existing config
    existing_config = load_config(config_path)

    # Remove server if it exists
    if name in existing_config.mcp_servers:
        del existing_config.mcp_servers[name]

        # Save config
        save_config(existing_config, config_path)
