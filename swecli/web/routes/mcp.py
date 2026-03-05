"""MCP (Model Context Protocol) API endpoints."""

from typing import Dict, Any, List, Optional
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from swecli.web.state import get_state, broadcast_to_all_clients
from swecli.core.context_engineering.mcp.config import get_config_path, get_project_config_path
from swecli.core.context_engineering.mcp.models import MCPServerConfig

router = APIRouter(prefix="/api/mcp", tags=["mcp"])


class MCPServerCreate(BaseModel):
    """Model for creating a new MCP server."""
    name: str
    command: str
    args: List[str] = []
    env: Dict[str, str] = {}
    enabled: bool = True
    auto_start: bool = False
    project_config: bool = False


class MCPServerUpdate(BaseModel):
    """Model for updating an MCP server."""
    command: Optional[str] = None
    args: Optional[List[str]] = None
    env: Optional[Dict[str, str]] = None
    enabled: Optional[bool] = None
    auto_start: Optional[bool] = None


@router.get("/servers")
async def list_servers() -> Dict[str, Any]:
    """List all configured MCP servers with their status.

    Returns:
        Dictionary containing list of servers with status and config
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            return {"servers": []}

        servers = state.mcp_manager.list_servers()
        result = []

        for name, config in servers.items():
            is_connected = state.mcp_manager.is_connected(name)
            tools = state.mcp_manager.get_server_tools(name) if is_connected else []

            # Get config location
            global_config = get_config_path()
            project_config = get_project_config_path(state.mcp_manager.working_dir)

            if project_config and project_config.exists():
                config_location = "project"
                config_path = str(project_config)
            else:
                config_location = "global"
                config_path = str(global_config)

            result.append({
                "name": name,
                "status": "connected" if is_connected else "disconnected",
                "config": {
                    "command": config.command,
                    "args": config.args,
                    "env": config.env,
                    "enabled": config.enabled,
                    "auto_start": config.auto_start,
                },
                "tools_count": len(tools),
                "config_location": config_location,
                "config_path": config_path,
            })

        return {"servers": result}

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to list servers: {str(e)}")


@router.get("/servers/{name}")
async def get_server(name: str) -> Dict[str, Any]:
    """Get detailed information about a specific MCP server.

    Args:
        name: Server name

    Returns:
        Detailed server information including tools and capabilities
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        servers = state.mcp_manager.list_servers()
        if name not in servers:
            raise HTTPException(status_code=404, detail=f"Server '{name}' not found")

        config = servers[name]
        is_connected = state.mcp_manager.is_connected(name)
        tools = state.mcp_manager.get_server_tools(name) if is_connected else []

        # Get capabilities
        capabilities = []
        if is_connected and tools:
            capabilities.append("tools")

        # Get config location
        global_config = get_config_path()
        project_config = get_project_config_path(state.mcp_manager.working_dir)

        if project_config and project_config.exists():
            config_path = str(project_config)
        else:
            config_path = str(global_config)

        # Transform tools to camelCase for frontend
        transformed_tools = []
        for tool in tools:
            transformed_tool = {
                "name": tool.get("mcp_tool_name", tool.get("name", "")),
                "description": tool.get("description", ""),
            }
            if "input_schema" in tool:
                transformed_tool["inputSchema"] = tool["input_schema"]
            transformed_tools.append(transformed_tool)

        return {
            "name": name,
            "status": "connected" if is_connected else "disconnected",
            "config": {
                "command": config.command,
                "args": config.args,
                "env": config.env,
                "enabled": config.enabled,
                "auto_start": config.auto_start,
            },
            "tools": transformed_tools,
            "capabilities": capabilities,
            "config_path": config_path,
        }

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to get server: {str(e)}")


@router.post("/servers/{name}/connect")
async def connect_server(name: str) -> Dict[str, Any]:
    """Connect to an MCP server.

    Args:
        name: Server name

    Returns:
        Connection result with tool count
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        if state.mcp_manager.is_connected(name):
            tools = state.mcp_manager.get_server_tools(name)
            return {
                "success": True,
                "message": f"Already connected to '{name}'",
                "tools_count": len(tools)
            }

        # Connect asynchronously
        success = await state.mcp_manager.connect(name)

        if success:
            tools = state.mcp_manager.get_server_tools(name)

            # Broadcast status change to all WebSocket clients
            await broadcast_to_all_clients({
                "type": "mcp:status_changed",
                "data": {
                    "server_name": name,
                    "status": "connected",
                    "tools_count": len(tools),
                }
            })

            return {
                "success": True,
                "message": f"Connected to '{name}'",
                "tools_count": len(tools)
            }
        else:
            return {
                "success": False,
                "message": f"Failed to connect to '{name}'",
                "tools_count": 0
            }

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Connection failed: {str(e)}")


@router.post("/servers/{name}/disconnect")
async def disconnect_server(name: str) -> Dict[str, Any]:
    """Disconnect from an MCP server.

    Args:
        name: Server name

    Returns:
        Disconnection result
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        if not state.mcp_manager.is_connected(name):
            return {
                "success": True,
                "message": f"Not connected to '{name}'"
            }

        # Disconnect synchronously (it's fast)
        state.mcp_manager.disconnect_sync(name)

        # Broadcast status change to all WebSocket clients
        await broadcast_to_all_clients({
            "type": "mcp:status_changed",
            "data": {
                "server_name": name,
                "status": "disconnected",
                "tools_count": 0,
            }
        })

        return {
            "success": True,
            "message": f"Disconnected from '{name}'"
        }

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Disconnection failed: {str(e)}")


@router.post("/servers/{name}/test")
async def test_server(name: str) -> Dict[str, Any]:
    """Test connection to an MCP server.

    Args:
        name: Server name

    Returns:
        Test result with tool count if successful
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        servers = state.mcp_manager.list_servers()
        if name not in servers:
            raise HTTPException(status_code=404, detail=f"Server '{name}' not found")

        # Try to connect
        success = await state.mcp_manager.connect(name)

        if success:
            tools = state.mcp_manager.get_server_tools(name)
            return {
                "success": True,
                "message": "Connection successful",
                "tools_count": len(tools)
            }
        else:
            return {
                "success": False,
                "message": "Connection failed",
                "tools_count": 0
            }

    except Exception as e:
        return {
            "success": False,
            "message": f"Test failed: {str(e)}",
            "tools_count": 0
        }


@router.post("/servers")
async def create_server(server: MCPServerCreate) -> Dict[str, Any]:
    """Add a new MCP server.

    Args:
        server: Server configuration

    Returns:
        Creation result
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        # Check if server already exists
        servers = state.mcp_manager.list_servers()
        if server.name in servers:
            raise HTTPException(status_code=400, detail=f"Server '{server.name}' already exists")

        # Create server config
        from swecli.core.context_engineering.mcp.config import save_server_config

        config = MCPServerConfig(
            command=server.command,
            args=server.args,
            env=server.env,
            enabled=server.enabled,
            auto_start=server.auto_start,
        )

        # Save to appropriate config file
        save_server_config(
            name=server.name,
            config=config,
            project_config=server.project_config,
            working_dir=state.mcp_manager.working_dir if server.project_config else None
        )

        # Reload configuration
        state.mcp_manager.load_configuration()

        # Broadcast server list update
        await broadcast_to_all_clients({
            "type": "mcp:servers_updated",
            "data": {
                "action": "added",
                "server_name": server.name,
            }
        })

        return {
            "success": True,
            "message": f"Server '{server.name}' added successfully"
        }

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to add server: {str(e)}")


@router.put("/servers/{name}")
async def update_server(name: str, update: MCPServerUpdate) -> Dict[str, Any]:
    """Update an MCP server configuration.

    Args:
        name: Server name
        update: Updated configuration

    Returns:
        Update result
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        servers = state.mcp_manager.list_servers()
        if name not in servers:
            raise HTTPException(status_code=404, detail=f"Server '{name}' not found")

        config = servers[name]

        # Update config fields
        if update.command is not None:
            config.command = update.command
        if update.args is not None:
            config.args = update.args
        if update.env is not None:
            config.env = update.env
        if update.enabled is not None:
            config.enabled = update.enabled
        if update.auto_start is not None:
            config.auto_start = update.auto_start

        # Save updated config
        from swecli.core.context_engineering.mcp.config import save_server_config, get_project_config_path

        # Determine if it's a project config
        project_config_path = get_project_config_path(state.mcp_manager.working_dir)
        is_project = project_config_path and project_config_path.exists()

        save_server_config(
            name=name,
            config=config,
            project_config=is_project,
            working_dir=state.mcp_manager.working_dir if is_project else None
        )

        # Reload configuration
        state.mcp_manager.load_configuration()

        # Broadcast server list update
        await broadcast_to_all_clients({
            "type": "mcp:servers_updated",
            "data": {
                "action": "updated",
                "server_name": name,
            }
        })

        return {
            "success": True,
            "message": f"Server '{name}' updated successfully"
        }

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to update server: {str(e)}")


@router.delete("/servers/{name}")
async def delete_server(name: str) -> Dict[str, Any]:
    """Remove an MCP server.

    Args:
        name: Server name

    Returns:
        Deletion result
    """
    try:
        state = get_state()
        if not state.mcp_manager:
            raise HTTPException(status_code=404, detail="MCP manager not available")

        servers = state.mcp_manager.list_servers()
        if name not in servers:
            raise HTTPException(status_code=404, detail=f"Server '{name}' not found")

        # Disconnect if connected
        if state.mcp_manager.is_connected(name):
            state.mcp_manager.disconnect_sync(name)

        # Remove from config
        from swecli.core.context_engineering.mcp.config import remove_server_config, get_project_config_path

        # Determine if it's a project config
        project_config_path = get_project_config_path(state.mcp_manager.working_dir)
        is_project = project_config_path and project_config_path.exists()

        remove_server_config(
            name=name,
            project_config=is_project,
            working_dir=state.mcp_manager.working_dir if is_project else None
        )

        # Reload configuration
        state.mcp_manager.load_configuration()

        # Broadcast server list update
        await broadcast_to_all_clients({
            "type": "mcp:servers_updated",
            "data": {
                "action": "removed",
                "server_name": name,
            }
        })

        return {
            "success": True,
            "message": f"Server '{name}' removed successfully"
        }

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to remove server: {str(e)}")
