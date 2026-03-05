/**
 * MCP (Model Context Protocol) types for the web UI.
 */

export type MCPServerStatus = 'connected' | 'disconnected' | 'connecting' | 'error';

export interface MCPServerConfig {
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  auto_start: boolean;
}

export interface MCPServer {
  name: string;
  status: MCPServerStatus;
  config: MCPServerConfig;
  tools_count: number;
  config_location: 'global' | 'project';
  config_path: string;
}

export interface MCPTool {
  name: string;
  description: string;
  inputSchema?: {
    type?: string;
    properties?: Record<string, {
      type?: string;
      description?: string;
      enum?: string[];
      items?: any;
      [key: string]: any;
    }>;
    required?: string[];
    [key: string]: any;
  };
}

export interface MCPServerDetailed extends MCPServer {
  tools: MCPTool[];
  capabilities: string[];
}

export interface MCPServerCreateRequest {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  enabled?: boolean;
  auto_start?: boolean;
  project_config?: boolean;
}

export interface MCPServerUpdateRequest {
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  enabled?: boolean;
  auto_start?: boolean;
}

export interface MCPApiResponse {
  success: boolean;
  message: string;
  tools_count?: number;
}

export interface MCPServersResponse {
  servers: MCPServer[];
}

// WebSocket event types
export interface MCPStatusChangedEvent {
  type: 'mcp:status_changed';
  data: {
    server_name: string;
    status: MCPServerStatus;
    tools_count: number;
  };
}

export interface MCPServersUpdatedEvent {
  type: 'mcp:servers_updated';
  data: {
    action: 'added' | 'removed' | 'updated';
    server_name: string;
  };
}

export type MCPWebSocketEvent = MCPStatusChangedEvent | MCPServersUpdatedEvent;
