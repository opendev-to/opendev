/**
 * MCP API client for managing MCP servers.
 *
 * This module provides a clean interface to interact with MCP server endpoints,
 * following the Single Responsibility Principle by focusing solely on API communication.
 */

import type {
  MCPServersResponse,
  MCPServerDetailed,
  MCPServerCreateRequest,
  MCPServerUpdateRequest,
  MCPApiResponse,
} from '../types/mcp';

const API_BASE = '/api';

/**
 * Helper function for making API requests
 */
async function fetchAPI<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${url}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    const errorData = await response.json().catch(() => ({ message: response.statusText }));
    throw new Error(errorData.message || `API error: ${response.statusText}`);
  }

  return response.json();
}

/**
 * List all configured MCP servers with their status.
 */
export async function listMCPServers(): Promise<MCPServersResponse> {
  return fetchAPI<MCPServersResponse>('/mcp/servers');
}

/**
 * Get detailed information about a specific MCP server.
 */
export async function getMCPServer(name: string): Promise<MCPServerDetailed> {
  return fetchAPI<MCPServerDetailed>(`/mcp/servers/${encodeURIComponent(name)}`);
}

/**
 * Connect to an MCP server.
 */
export async function connectMCPServer(name: string): Promise<MCPApiResponse> {
  return fetchAPI<MCPApiResponse>(
    `/mcp/servers/${encodeURIComponent(name)}/connect`,
    { method: 'POST' }
  );
}

/**
 * Disconnect from an MCP server.
 */
export async function disconnectMCPServer(name: string): Promise<MCPApiResponse> {
  return fetchAPI<MCPApiResponse>(
    `/mcp/servers/${encodeURIComponent(name)}/disconnect`,
    { method: 'POST' }
  );
}

/**
 * Test connection to an MCP server.
 */
export async function testMCPServer(name: string): Promise<MCPApiResponse> {
  return fetchAPI<MCPApiResponse>(
    `/mcp/servers/${encodeURIComponent(name)}/test`,
    { method: 'POST' }
  );
}

/**
 * Create a new MCP server configuration.
 */
export async function createMCPServer(server: MCPServerCreateRequest): Promise<MCPApiResponse> {
  return fetchAPI<MCPApiResponse>('/mcp/servers', {
    method: 'POST',
    body: JSON.stringify(server),
  });
}

/**
 * Update an existing MCP server configuration.
 */
export async function updateMCPServer(
  name: string,
  update: MCPServerUpdateRequest
): Promise<MCPApiResponse> {
  return fetchAPI<MCPApiResponse>(
    `/mcp/servers/${encodeURIComponent(name)}`,
    {
      method: 'PUT',
      body: JSON.stringify(update),
    }
  );
}

/**
 * Delete an MCP server configuration.
 */
export async function deleteMCPServer(name: string): Promise<MCPApiResponse> {
  return fetchAPI<MCPApiResponse>(
    `/mcp/servers/${encodeURIComponent(name)}`,
    { method: 'DELETE' }
  );
}
