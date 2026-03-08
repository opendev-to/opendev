// Tool call information
export interface ToolCallInfo {
  id: string;
  name: string;
  parameters: Record<string, any>;
  result?: string | null;
  error?: string | null;
  result_summary?: string | null;
  approved?: boolean | null;
  nested_tool_calls?: ToolCallInfo[] | null;
}

// Message types
export interface Message {
  role: 'user' | 'assistant' | 'system' | 'tool_call' | 'tool_result' | 'thinking';
  content: string;
  timestamp?: string;
  tool_call_id?: string;
  tool_name?: string;
  tool_args?: Record<string, any>;
  tool_result?: any;
  tool_args_display?: string | null;
  tool_summary?: string | string[] | null;
  tool_success?: boolean;
  tool_error?: string | null;
  tool_calls?: ToolCallInfo[];
  metadata?: Record<string, any>;
  depth?: number;
  parent_tool_call_id?: string;
  thinking_trace?: string | null;
  reasoning_content?: string | null;
}

// Session types
export interface Session {
  id: string;
  working_dir: string;  // Backend returns this key even though model has working_directory
  created_at: string;
  updated_at: string;
  message_count: number;
  token_usage: Record<string, number>;
  title?: string;
  has_session_model?: boolean;
}

// Configuration types
export interface Config {
  model_provider: string;
  model: string;
  api_key: string | null;
  temperature: number;
  enable_bash: boolean;
  working_directory: string;
}

// Provider types
export interface Model {
  id: string;
  name: string;
  description: string;
}

export interface Provider {
  id: string;
  name: string;
  description: string;
  models: Model[];
}

// WebSocket event types
export interface WSMessage {
  type: 'user_message' | 'message_start' | 'message_chunk' | 'message_complete' | 'tool_call' | 'tool_result' | 'approval_required' | 'approval_resolved' | 'error' | 'pong' | 'mcp_status_update' | 'mcp_servers_update' | 'connected' | 'disconnected' | 'thinking_block' | 'status_update' | 'ask_user_required' | 'ask_user_resolved' | 'session_activity' | 'plan_approval_required' | 'plan_approval_resolved' | 'plan_content' | 'subagent_start' | 'subagent_complete' | 'parallel_agents_start' | 'parallel_agents_done' | 'task_completed' | 'progress' | 'nested_tool_call' | 'nested_tool_result';
  data: any;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, any>;
  requiresApproval: boolean;
}

export interface ApprovalRequest {
  id: string;
  tool_name: string;
  arguments: Record<string, any>;
  description: string;
  preview?: string;
}

// Status bar info
export interface StatusInfo {
  mode: 'normal' | 'plan';
  autonomy_level: 'Manual' | 'Semi-Auto' | 'Auto';
  thinking_level?: 'Off' | 'Low' | 'Medium' | 'High';
  model?: string;
  model_provider?: string;
  working_dir?: string;
  git_branch?: string | null;
  session_cost?: number;
  context_usage_pct?: number;
}

// Ask-user question types
export interface AskUserOption {
  label: string;
  description: string;
}

export interface AskUserQuestion {
  question: string;
  header: string;
  options: AskUserOption[];
  multi_select: boolean;
}

export interface AskUserRequest {
  request_id: string;
  questions: AskUserQuestion[];
}

// Plan approval types
export interface PlanApprovalRequest {
  request_id: string;
  plan_content: string;
}

// Per-session state for concurrent session support
export interface PerSessionState {
  messages: Message[];
  isLoading: boolean;
  error: string | null;
  pendingApproval: ApprovalRequest | null;
  pendingAskUser: AskUserRequest | null;
  pendingPlanApproval: PlanApprovalRequest | null;
  progressMessage: string | null;
  queuedMessages: string[];
}
