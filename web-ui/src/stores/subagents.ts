import { create } from 'zustand';
import { wsClient } from '../api/websocket';

export interface ActiveToolCall {
  toolName: string;
  toolId: string;
  args: Record<string, any>;
  startedAt: number; // Date.now()
}

export interface CompletedToolCall {
  toolName: string;
  args: Record<string, any>;
  elapsedMs: number;
  success: boolean;
}

export interface SubagentState {
  subagentId: string;
  name: string;
  task: string;
  description: string;
  startedAt: number;
  finished: boolean;
  success: boolean;
  resultSummary: string;
  toolCallCount: number;
  activeTools: Map<string, ActiveToolCall>;
  completedTools: CompletedToolCall[];
  tokenCount: number;
  cumulativeOutputTokens: number;
  shallowWarning: string | null;
  toolCallId: string | null; // parent tool_call_id from spawn_subagent
}

interface SubagentStore {
  subagents: Map<string, SubagentState>;
  // Ordered list of subagent IDs for display
  order: string[];
}

export const useSubagentStore = create<SubagentStore>(() => ({
  subagents: new Map(),
  order: [],
}));

function formatToolVerb(toolName: string): string {
  const map: Record<string, string> = {
    'read_file': 'Read', 'write_file': 'Write', 'edit_file': 'Edit',
    'search_code': 'Search', 'search': 'Search', 'run_command': 'Bash',
    'bash_execute': 'Bash', 'list_files': 'List', 'list_directory': 'List',
    'fetch_url': 'Fetch', 'find_symbol': 'Find Symbol', 'web_search': 'Search',
    'apply_patch': 'Patch', 'delete_file': 'Delete', 'git': 'Git',
  };
  return map[toolName] || toolName.split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
}

function formatToolArg(toolName: string, args: Record<string, any>): string {
  const keys: Record<string, string[]> = {
    'read_file': ['file_path', 'path'], 'write_file': ['file_path', 'path'],
    'edit_file': ['file_path', 'path'], 'search_code': ['pattern', 'query'],
    'search': ['pattern', 'query'], 'run_command': ['command'],
    'bash_execute': ['command'], 'list_files': ['path', 'directory'],
    'fetch_url': ['url'], 'web_search': ['query'],
  };
  for (const key of (keys[toolName] || Object.keys(args))) {
    if (args[key] && typeof args[key] === 'string') {
      const val = args[key];
      return val.length > 50 ? val.slice(0, 47) + '...' : val;
    }
  }
  return '';
}

export { formatToolVerb, formatToolArg };

// ─── WebSocket Event Handlers ───────────────────────────────────────────────

wsClient.on('subagent_start', (message) => {
  const d = message.data;
  if (!d) return;

  const id = d.subagent_id || d.tool_call_id || `sa-${Date.now()}`;
  const name = d.agent_type || d.subagent_name || 'Agent';
  const task = d.task || d.description || '';

  const sa: SubagentState = {
    subagentId: id,
    name,
    task,
    description: d.description || task,
    startedAt: Date.now(),
    finished: false,
    success: false,
    resultSummary: '',
    toolCallCount: 0,
    activeTools: new Map(),
    completedTools: [],
    tokenCount: 0,
    cumulativeOutputTokens: 0,
    shallowWarning: null,
    toolCallId: d.tool_call_id || null,
  };

  useSubagentStore.setState((state) => {
    const subagents = new Map(state.subagents);
    // Clear all finished subagents when a new batch starts
    const allFinished = state.order.length > 0 && state.order.every(sid => {
      const s = subagents.get(sid);
      return s?.finished;
    });
    if (allFinished) {
      subagents.clear();
      return { subagents: new Map([[id, sa]]), order: [id] };
    }

    subagents.set(id, sa);
    return { subagents, order: [...state.order.filter(sid => subagents.has(sid)), id] };
  });
});

wsClient.on('nested_tool_call', (message) => {
  const d = message.data;
  if (!d) return;

  // Try to find the subagent this tool belongs to
  const state = useSubagentStore.getState();
  // nested_tool_call may have a subagent_id or parent field
  const subagentId = d.subagent_id || d.parent_subagent_id;

  if (subagentId && state.subagents.has(subagentId)) {
    const toolId = d.tool_call_id || d.tool_id || `t-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    useSubagentStore.setState((prev) => {
      const subagents = new Map(prev.subagents);
      const sa = subagents.get(subagentId);
      if (!sa || sa.finished) return {};

      const activeTools = new Map(sa.activeTools);
      activeTools.set(toolId, {
        toolName: d.tool_name || 'unknown',
        toolId,
        args: d.arguments || d.args || {},
        startedAt: Date.now(),
      });

      subagents.set(subagentId, {
        ...sa,
        activeTools,
        toolCallCount: sa.toolCallCount + 1,
      });
      return { subagents };
    });
  }
});

wsClient.on('nested_tool_result', (message) => {
  const d = message.data;
  if (!d) return;

  const subagentId = d.subagent_id || d.parent_subagent_id;

  if (subagentId) {
    useSubagentStore.setState((prev) => {
      const subagents = new Map(prev.subagents);
      const sa = subagents.get(subagentId);
      if (!sa) return {};

      const activeTools = new Map(sa.activeTools);
      const toolId = d.tool_call_id || d.tool_id;

      // Find by tool_id or by tool_name match
      let matchedId = toolId && activeTools.has(toolId) ? toolId : null;
      if (!matchedId) {
        for (const [tid, tc] of activeTools) {
          if (tc.toolName === d.tool_name) {
            matchedId = tid;
            break;
          }
        }
      }

      if (matchedId) {
        const tc = activeTools.get(matchedId)!;
        activeTools.delete(matchedId);
        const completedTools = [...sa.completedTools, {
          toolName: tc.toolName,
          args: tc.args,
          elapsedMs: Date.now() - tc.startedAt,
          success: d.success !== false,
        }];
        // Cap at 50
        if (completedTools.length > 50) {
          completedTools.splice(0, completedTools.length - 50);
        }

        subagents.set(subagentId, { ...sa, activeTools, completedTools });
      }

      return { subagents };
    });
  }
});

wsClient.on('subagent_complete', (message) => {
  const d = message.data;
  if (!d) return;

  const id = d.subagent_id || d.tool_call_id;
  if (!id) return;

  useSubagentStore.setState((prev) => {
    const subagents = new Map(prev.subagents);

    // Try to find by subagent_id first, then by tool_call_id
    let sa = subagents.get(id);
    let matchedId = id;
    if (!sa) {
      for (const [sid, s] of subagents) {
        if (s.toolCallId === id) {
          sa = s;
          matchedId = sid;
          break;
        }
      }
    }
    if (!sa) return {};

    subagents.set(matchedId, {
      ...sa,
      finished: true,
      success: d.success !== false,
      resultSummary: d.result_summary || d.summary || (d.success !== false ? 'Completed' : 'Failed'),
      toolCallCount: d.tool_call_count || sa.toolCallCount,
      shallowWarning: d.shallow_warning || null,
      activeTools: new Map(),
      completedTools: [],
    });

    return { subagents };
  });
});

// Token updates (if backend sends them)
wsClient.on('status_update', (message) => {
  const d = message.data;
  if (!d?.subagent_id || (d.input_tokens == null && d.output_tokens == null)) return;

  useSubagentStore.setState((prev) => {
    const subagents = new Map(prev.subagents);
    const sa = subagents.get(d.subagent_id);
    if (!sa) return {};

    // Input tokens replaced (each call sends full context), output tokens accumulated
    const cumOut = sa.cumulativeOutputTokens + (d.output_tokens || 0);
    subagents.set(d.subagent_id, {
      ...sa,
      tokenCount: (d.input_tokens || 0) + cumOut,
      cumulativeOutputTokens: cumOut,
    });
    return { subagents };
  });
});
