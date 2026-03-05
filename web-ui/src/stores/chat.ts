import { create } from 'zustand';
import type { Message, ApprovalRequest, StatusInfo, AskUserRequest, PlanApprovalRequest, PerSessionState } from '../types';
import { apiClient } from '../api/client';
import { wsClient } from '../api/websocket';

// ─── Helpers ────────────────────────────────────────────────────────────────

const DEFAULT_SESSION: PerSessionState = {
  messages: [],
  isLoading: false,
  error: null,
  pendingApproval: null,
  pendingAskUser: null,
  pendingPlanApproval: null,
};

function getSessionState(states: Record<string, PerSessionState>, id: string): PerSessionState {
  return states[id] ?? DEFAULT_SESSION;
}

function patchSession(
  state: ChatState,
  sessionId: string,
  patch: Partial<PerSessionState> | ((prev: PerSessionState) => Partial<PerSessionState>),
): { sessionStates: Record<string, PerSessionState> } {
  const prev = getSessionState(state.sessionStates, sessionId);
  const updates = typeof patch === 'function' ? patch(prev) : patch;
  return {
    sessionStates: {
      ...state.sessionStates,
      [sessionId]: { ...prev, ...updates },
    },
  };
}

/** Expand raw API messages (with tool_calls arrays) into flat message list. */
function expandMessages(rawMessages: Message[]): Message[] {
  const expanded: Message[] = [];
  for (const msg of rawMessages) {
    if (msg.tool_calls && msg.tool_calls.length > 0) {
      if (msg.content && msg.content.trim()) {
        expanded.push({
          role: msg.role as any,
          content: msg.content,
          timestamp: msg.timestamp,
        });
      }
      for (const tc of msg.tool_calls) {
        const toolResult = tc.error
          ? { success: false, error: tc.error }
          : tc.result ?? '';
        expanded.push({
          role: 'tool_call',
          content: `Calling ${tc.name}`,
          tool_call_id: tc.id,
          tool_name: tc.name,
          tool_args: tc.parameters,
          tool_args_display: undefined,
          tool_result: toolResult,
          tool_summary: tc.result_summary || null,
          tool_success: !tc.error,
          tool_error: tc.error || null,
          timestamp: msg.timestamp,
        });
      }
    } else {
      if (msg.content && msg.content.trim()) {
        expanded.push({
          role: msg.role as any,
          content: msg.content,
          timestamp: msg.timestamp,
        });
      }
    }
  }
  return expanded;
}

// ─── Store Interface ────────────────────────────────────────────────────────

interface ChatState {
  // Per-session state (the big change)
  sessionStates: Record<string, PerSessionState>;

  // Global state
  isConnected: boolean;
  currentSessionId: string | null;
  hasWorkspace: boolean;
  status: StatusInfo | null;
  thinkingLevel: 'Off' | 'Low' | 'Medium' | 'High';
  runningSessions: Set<string>;
  sessionListVersion: number;
  sidebarCollapsed: boolean;

  // Actions
  loadSession: (sessionId: string) => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  clearChat: () => Promise<void>;
  setConnected: (connected: boolean) => void;
  respondToApproval: (approvalId: string, approved: boolean, autoApprove?: boolean) => void;
  setHasWorkspace: (hasWorkspace: boolean) => void;
  setStatus: (status: StatusInfo) => void;
  toggleMode: () => void;
  cycleAutonomy: () => void;
  cycleThinkingLevel: () => void;
  respondToAskUser: (requestId: string, answers: Record<string, any> | null) => void;
  respondToPlanApproval: (requestId: string, action: string, feedback?: string) => void;
  sendInterrupt: () => void;
  bumpSessionList: () => void;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
}

const AUTONOMY_CYCLE: Array<'Manual' | 'Semi-Auto' | 'Auto'> = ['Manual', 'Semi-Auto', 'Auto'];
const THINKING_CYCLE: Array<'Off' | 'Low' | 'Medium' | 'High'> = ['Off', 'Low', 'Medium', 'High'];

export const useChatStore = create<ChatState>((set, get) => ({
  sessionStates: {},
  isConnected: false,
  currentSessionId: null,
  hasWorkspace: false,
  status: null,
  thinkingLevel: 'Medium',
  runningSessions: new Set<string>(),
  sessionListVersion: 0,
  sidebarCollapsed: false,

  bumpSessionList: () => set(state => ({ sessionListVersion: state.sessionListVersion + 1 })),
  toggleSidebar: () => set(state => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),

  loadSession: async (sessionId: string) => {
    console.log(`[Frontend] Loading session ${sessionId}`);

    const existing = get().sessionStates[sessionId];
    if (existing && existing.messages.length > 0) {
      // Instant switch — already cached
      console.log(`[Frontend] Session ${sessionId} cached (${existing.messages.length} msgs), instant switch`);
      set({ currentSessionId: sessionId, hasWorkspace: true });
    } else {
      // Need to fetch messages
      set(state => ({
        currentSessionId: sessionId,
        hasWorkspace: true,
        ...patchSession(state, sessionId, { isLoading: true, error: null }),
      }));

      try {
        console.log(`[Frontend] Fetching messages for session ${sessionId}`);
        const rawMessages = await apiClient.getSessionMessages(sessionId);
        const messages = expandMessages(rawMessages);
        console.log(`[Frontend] Loaded ${messages.length} messages for ${sessionId}`);

        set(state => ({
          ...patchSession(state, sessionId, { messages, isLoading: false }),
        }));
      } catch (error) {
        console.error(`[Frontend] Failed to load session ${sessionId}:`, error);
        set(state => ({
          ...patchSession(state, sessionId, {
            error: error instanceof Error ? error.message : 'Failed to load session',
            isLoading: false,
          }),
        }));
      }
    }

    // Fire-and-forget: resume on backend for config context
    apiClient.resumeSession(sessionId).catch(() => {});

    // Refresh status after session change
    try {
      const configData = await apiClient.getConfig();
      set({
        thinkingLevel: configData.thinking_level || 'Medium',
        status: {
          mode: configData.mode || 'normal',
          autonomy_level: configData.autonomy_level || 'Manual',
          thinking_level: configData.thinking_level || 'Medium',
          model: configData.model,
          model_provider: configData.model_provider,
          working_dir: configData.working_dir || '',
          git_branch: configData.git_branch,
        },
      });
    } catch (_) { /* ignore */ }

    console.log(`[Frontend] Session ${sessionId} loaded successfully`);
  },

  sendMessage: async (content: string) => {
    const sessionId = get().currentSessionId;
    if (!sessionId) return;

    const userMessage: Message = {
      role: 'user',
      content,
      timestamp: new Date().toISOString(),
    };

    set(state => ({
      ...patchSession(state, sessionId, prev => ({
        messages: [...prev.messages, userMessage],
        isLoading: true,
        error: null,
      })),
    }));

    try {
      wsClient.send({
        type: 'query',
        data: { message: content, session_id: sessionId },
      });
    } catch (error) {
      set(state => ({
        ...patchSession(state, sessionId, {
          error: error instanceof Error ? error.message : 'Failed to send message',
          isLoading: false,
        }),
      }));
    }
  },

  clearChat: async () => {
    const sessionId = get().currentSessionId;
    try {
      await apiClient.clearChat();
      if (sessionId) {
        set(state => ({
          ...patchSession(state, sessionId, { messages: [], error: null }),
        }));
      }
    } catch (error) {
      if (sessionId) {
        set(state => ({
          ...patchSession(state, sessionId, {
            error: error instanceof Error ? error.message : 'Failed to clear chat',
          }),
        }));
      }
    }
  },

  setConnected: (connected: boolean) => {
    set({ isConnected: connected });
  },

  respondToApproval: (approvalId: string, approved: boolean, autoApprove: boolean = false) => {
    wsClient.send({
      type: 'approve',
      data: { approvalId, approved, autoApprove },
    });
    const sessionId = get().currentSessionId;
    if (sessionId) {
      set(state => ({
        ...patchSession(state, sessionId, { pendingApproval: null }),
      }));
    }
  },

  setHasWorkspace: (hasWorkspace: boolean) => {
    set({ hasWorkspace });
  },

  setStatus: (status: StatusInfo) => {
    set({ status });
  },

  toggleMode: () => {
    const { status } = get();
    if (!status) return;
    const newMode = status.mode === 'normal' ? 'plan' : 'normal';
    apiClient.setMode(newMode).catch(console.error);
    set({ status: { ...status, mode: newMode } });
  },

  cycleAutonomy: () => {
    const { status } = get();
    if (!status) return;
    const currentIdx = AUTONOMY_CYCLE.indexOf(status.autonomy_level);
    const nextLevel = AUTONOMY_CYCLE[(currentIdx + 1) % AUTONOMY_CYCLE.length];
    apiClient.setAutonomy(nextLevel).catch(console.error);
    set({ status: { ...status, autonomy_level: nextLevel } });
  },

  cycleThinkingLevel: () => {
    const { status } = get();
    const currentLevel = status?.thinking_level || get().thinkingLevel;
    const currentIdx = THINKING_CYCLE.indexOf(currentLevel as any);
    const nextLevel = THINKING_CYCLE[(currentIdx + 1) % THINKING_CYCLE.length];
    apiClient.setThinkingLevel(nextLevel).catch(console.error);
    set({
      thinkingLevel: nextLevel,
      status: status ? { ...status, thinking_level: nextLevel } : status,
    });
  },

  respondToAskUser: (requestId: string, answers: Record<string, any> | null) => {
    wsClient.send({
      type: 'ask_user_response',
      data: { requestId, answers, cancelled: answers === null },
    });
    const sessionId = get().currentSessionId;
    if (sessionId) {
      set(state => ({
        ...patchSession(state, sessionId, { pendingAskUser: null }),
      }));
    }
  },

  respondToPlanApproval: (requestId: string, action: string, feedback?: string) => {
    wsClient.send({
      type: 'plan_approval_response',
      data: { requestId, action, feedback: feedback || '' },
    });
    const sessionId = get().currentSessionId;
    if (sessionId) {
      set(state => ({
        ...patchSession(state, sessionId, { pendingPlanApproval: null }),
      }));
    }
  },

  sendInterrupt: () => {
    const sessionId = get().currentSessionId;
    if (!sessionId) return;

    apiClient.interruptTask().catch(console.error);

    set(state => ({
      ...patchSession(state, sessionId, prev => ({
        isLoading: false,
        pendingApproval: null,
        pendingAskUser: null,
        pendingPlanApproval: null,
        messages: markLastToolCallInterrupted(prev.messages),
      })),
    }));
  },
}));

/** Mark the last pending tool_call message as interrupted. */
function markLastToolCallInterrupted(messages: Message[]): Message[] {
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === 'tool_call' && !messages[i].tool_result) {
      const updated = [...messages];
      updated[i] = {
        ...messages[i],
        tool_result: { success: false, error: 'Interrupted' },
        tool_success: false,
        tool_error: 'Interrupted',
      };
      return updated;
    }
  }
  return messages;
}

// ─── WebSocket Event Handlers ───────────────────────────────────────────────

/** Resolve the session ID from a WS event, falling back to currentSessionId. */
function resolveSessionId(data: any): string | null {
  return data?.session_id || useChatStore.getState().currentSessionId;
}

wsClient.on('connected', () => {
  useChatStore.getState().setConnected(true);
});

wsClient.on('disconnected', () => {
  useChatStore.getState().setConnected(false);
});

wsClient.on('user_message', () => {
  // Message already added optimistically by sendMessage
});

wsClient.on('message_start', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { isLoading: true }),
  }));
});

wsClient.on('message_chunk', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  console.log('[Frontend] Received message_chunk:', message.data.content.substring(0, 100));

  useChatStore.setState(state => {
    const sessionState = getSessionState(state.sessionStates, sid);
    const msgs = sessionState.messages;
    const lastMessage = msgs[msgs.length - 1];

    let newMessages: Message[];
    if (lastMessage && lastMessage.role === 'assistant') {
      newMessages = [
        ...msgs.slice(0, -1),
        { ...lastMessage, content: lastMessage.content + message.data.content },
      ];
    } else {
      newMessages = [
        ...msgs,
        { role: 'assistant' as const, content: message.data.content },
      ];
    }

    return patchSession(state, sid, { messages: newMessages });
  });
});

wsClient.on('message_complete', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  console.log('[Frontend] Received message_complete');
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { isLoading: false }),
  }));
});

wsClient.on('error', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  useChatStore.setState(state => ({
    ...patchSession(state, sid, {
      error: message.data.message,
      isLoading: false,
    }),
  }));
});

wsClient.on('approval_required', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  console.log('[Frontend] Received approval_required:', message.data);
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { pendingApproval: message.data as ApprovalRequest }),
  }));
});

wsClient.on('approval_resolved', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { pendingApproval: null }),
  }));
});

wsClient.on('tool_call', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;

  const toolCallMessage: Message = {
    role: 'tool_call',
    content: message.data.description || `Calling ${message.data.tool_name}`,
    tool_call_id: message.data.tool_call_id,
    tool_name: message.data.tool_name,
    tool_args: message.data.arguments,
    tool_args_display: message.data.arguments_display || null,
    timestamp: new Date().toISOString(),
  };

  useChatStore.setState(state => {
    const sessionState = getSessionState(state.sessionStates, sid);
    return patchSession(state, sid, { messages: [...sessionState.messages, toolCallMessage] });
  });
});

wsClient.on('tool_result', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;

  useChatStore.setState(state => {
    const sessionState = getSessionState(state.sessionStates, sid);
    const msgs = sessionState.messages;
    const callId = message.data.tool_call_id;

    for (let i = msgs.length - 1; i >= 0; i--) {
      if (
        msgs[i].role === 'tool_call' &&
        msgs[i].tool_call_id === callId &&
        !msgs[i].tool_result
      ) {
        const updatedMessages = [...msgs];
        updatedMessages[i] = {
          ...msgs[i],
          tool_result: message.data.raw_result ?? message.data.output,
          tool_summary: message.data.summary,
          tool_success: message.data.success,
          tool_error: message.data.error || null,
        };
        return patchSession(state, sid, { messages: updatedMessages });
      }
    }

    console.warn(`Received tool_result for ${message.data.tool_name} but no matching tool_call found`);
    return {};
  });
});

wsClient.on('thinking_block', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;

  useChatStore.setState(state => {
    const sessionState = getSessionState(state.sessionStates, sid);
    return patchSession(state, sid, {
      messages: [
        ...sessionState.messages,
        {
          role: 'thinking' as const,
          content: message.data.content,
          metadata: { level: message.data.level },
        },
      ],
    });
  });
});

wsClient.on('status_update', (message) => {
  const { status } = useChatStore.getState();
  const newStatus = {
    ...status,
    ...message.data,
  } as StatusInfo;
  useChatStore.setState({
    status: newStatus,
    thinkingLevel: newStatus.thinking_level || useChatStore.getState().thinkingLevel,
  });
});

wsClient.on('ask_user_required', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  console.log('[Frontend] Received ask_user_required:', message.data);
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { pendingAskUser: message.data as AskUserRequest }),
  }));
});

wsClient.on('ask_user_resolved', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { pendingAskUser: null }),
  }));
});

wsClient.on('session_activity', (message) => {
  const { session_id, status } = message.data;
  useChatStore.setState((state) => {
    const next = new Set(state.runningSessions);
    if (status === 'running') next.add(session_id);
    else next.delete(session_id);
    return { runningSessions: next };
  });
  useChatStore.getState().bumpSessionList();
});

// ─── Plan Approval Events ────────────────────────────────────────────────────

wsClient.on('plan_approval_required', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  console.log('[Frontend] Received plan_approval_required:', message.data);
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { pendingPlanApproval: message.data as PlanApprovalRequest }),
  }));
});

wsClient.on('plan_approval_resolved', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  useChatStore.setState(state => ({
    ...patchSession(state, sid, { pendingPlanApproval: null }),
  }));
});

// ─── Subagent Events ─────────────────────────────────────────────────────────

wsClient.on('subagent_start', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  const { agent_type, description, tool_call_id } = message.data;
  console.log('[Frontend] Subagent start:', agent_type, description);

  const subagentMessage: Message = {
    role: 'tool_call',
    content: `Spawning ${agent_type} agent`,
    tool_call_id: tool_call_id,
    tool_name: 'spawn_subagent',
    tool_args: { agent_type, description },
    timestamp: new Date().toISOString(),
  };

  useChatStore.setState(state => {
    const sessionState = getSessionState(state.sessionStates, sid);
    return patchSession(state, sid, { messages: [...sessionState.messages, subagentMessage] });
  });
});

wsClient.on('subagent_complete', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  const { tool_call_id, success } = message.data;

  useChatStore.setState(state => {
    const sessionState = getSessionState(state.sessionStates, sid);
    const msgs = sessionState.messages;

    for (let i = msgs.length - 1; i >= 0; i--) {
      if (
        msgs[i].role === 'tool_call' &&
        msgs[i].tool_name === 'spawn_subagent' &&
        msgs[i].tool_call_id === tool_call_id &&
        !msgs[i].tool_result
      ) {
        const updatedMessages = [...msgs];
        updatedMessages[i] = {
          ...msgs[i],
          tool_result: { success, output: success ? 'Agent completed' : 'Agent failed' },
          tool_summary: success ? 'Agent completed successfully' : 'Agent failed',
          tool_success: success,
        };
        return patchSession(state, sid, { messages: updatedMessages });
      }
    }
    return {};
  });
});

wsClient.on('task_completed', (message) => {
  const sid = resolveSessionId(message.data);
  if (!sid) return;
  console.log('[Frontend] Task completed:', message.data.summary);
});
