import type { Message, Session } from '../types';

const API_BASE = '/api';

class APIClient {
  // Chat endpoints
  async sendQuery(message: string, sessionId?: string): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/chat/query`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message, sessionId }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async getMessages(): Promise<Message[]> {
    const response = await fetch(`${API_BASE}/chat/messages`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async clearChat(): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/chat/clear`, {
      method: 'DELETE',
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  // Generic GET method for any endpoint
  async get<T = any>(endpoint: string): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async interruptTask(): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/chat/interrupt`, {
      method: 'POST',
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  // Session endpoints
  async listSessions(): Promise<Session[]> {
    const response = await fetch(`${API_BASE}/sessions`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async getCurrentSession(): Promise<Session> {
    const response = await fetch(`${API_BASE}/sessions/current`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async resumeSession(sessionId: string): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/sessions/${sessionId}/resume`, {
      method: 'POST',
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async exportSession(sessionId: string): Promise<any> {
    const response = await fetch(`${API_BASE}/sessions/${sessionId}/export`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async verifyPath(path: string): Promise<{ exists: boolean; is_directory: boolean; path?: string; error?: string }> {
    const response = await fetch(`${API_BASE}/sessions/verify-path`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async browseDirectory(path: string = '', showHidden: boolean = false): Promise<{
    current_path: string;
    parent_path: string | null;
    directories: Array<{ name: string; path: string }>;
    error: string | null;
  }> {
    const response = await fetch(`${API_BASE}/sessions/browse-directory`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path, show_hidden: showHidden }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async getSessionMessages(sessionId: string): Promise<Message[]> {
    const response = await fetch(`${API_BASE}/sessions/${sessionId}/messages`);
    if (!response.ok) {
      if (response.status === 404) return [];
      throw new Error(`API error: ${response.statusText}`);
    }
    return response.json();
  }

  async createSession(workspace: string): Promise<{ id: string; status: string; message?: string }> {
    const response = await fetch(`${API_BASE}/sessions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ working_directory: workspace }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  // Session model endpoints
  async getSessionModel(sessionId: string): Promise<Record<string, string>> {
    const response = await fetch(`${API_BASE}/sessions/${sessionId}/model`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async updateSessionModel(sessionId: string, overlay: Record<string, string | null>): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/sessions/${sessionId}/model`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(overlay),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async clearSessionModel(sessionId: string): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/sessions/${sessionId}/model`, {
      method: 'DELETE',
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async verifyModel(provider: string, model: string): Promise<{ valid: boolean; error?: string }> {
    const response = await fetch(`${API_BASE}/config/verify-model`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ provider, model }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  // Config endpoints
  async getConfig(): Promise<any> {
    const response = await fetch(`${API_BASE}/config`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async updateConfig(config: any): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/config`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async listProviders(): Promise<any[]> {
    const response = await fetch(`${API_BASE}/config/providers`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async setMode(mode: string): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/config/mode`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mode }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async setAutonomy(level: string): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/config/autonomy`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ level }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  async setThinkingLevel(level: string): Promise<{ status: string; message: string }> {
    const response = await fetch(`${API_BASE}/config/thinking`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ level }),
    });
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  // File listing
  async listFiles(query?: string): Promise<{ files: Array<{ path: string; name: string; is_file: boolean }> }> {
    const url = query ? `${API_BASE}/sessions/files?query=${encodeURIComponent(query)}` : `${API_BASE}/sessions/files`;
    const response = await fetch(url);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }

  // Bridge mode
  async getBridgeInfo(): Promise<{ bridge_mode: boolean; session_id: string | null }> {
    const response = await fetch(`${API_BASE}/sessions/bridge-info`);
    if (!response.ok) return { bridge_mode: false, session_id: null };
    return response.json();
  }

  // Health check
  async health(): Promise<{ status: string; service: string }> {
    const response = await fetch(`${API_BASE}/health`);
    if (!response.ok) throw new Error(`API error: ${response.statusText}`);
    return response.json();
  }
}

export const apiClient = new APIClient();
