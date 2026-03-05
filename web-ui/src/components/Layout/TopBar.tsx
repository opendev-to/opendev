import { useEffect } from 'react';
import { PanelLeft } from 'lucide-react';
import { useChatStore } from '../../stores/chat';
import { apiClient } from '../../api/client';

const MODE_STYLES = {
  normal: 'bg-bg-400/40 text-text-200 border-beige-300 hover:bg-bg-400/60',
  plan: 'bg-accent-secondary-900 text-accent-secondary-100 border-accent-secondary-900/50 hover:bg-accent-secondary-900/80',
} as const;

const AUTONOMY_STYLES = {
  'Manual': 'bg-bg-400/40 text-text-200 border-beige-300 hover:bg-bg-400/60',
  'Semi-Auto': 'bg-accent-secondary-900 text-accent-secondary-100 border-accent-secondary-900/50 hover:bg-accent-secondary-900/80',
  'Auto': 'bg-success-100/10 text-success-100 border-success-100/20 hover:bg-success-100/15',
} as const;

const THINKING_STYLES: Record<string, string> = {
  'Off':           'bg-bg-200 text-text-500 border-beige-300 hover:bg-bg-300',
  'Low':           'bg-cyan-500/10 text-cyan-600 border-cyan-500/20 hover:bg-cyan-500/15',
  'Medium':        'bg-success-100/10 text-success-100 border-success-100/20 hover:bg-success-100/15',
  'High':          'bg-yellow-500/10 text-yellow-600 border-yellow-500/20 hover:bg-yellow-500/15',
} as const;

export function TopBar() {
  const status = useChatStore(state => state.status);
  const isConnected = useChatStore(state => state.isConnected);
  const thinkingLevel = useChatStore(state => state.thinkingLevel);
  const sidebarCollapsed = useChatStore(state => state.sidebarCollapsed);
  const toggleMode = useChatStore(state => state.toggleMode);
  const cycleAutonomy = useChatStore(state => state.cycleAutonomy);
  const cycleThinkingLevel = useChatStore(state => state.cycleThinkingLevel);
  const toggleSidebar = useChatStore(state => state.toggleSidebar);

  // Load initial config on mount
  useEffect(() => {
    const loadStatus = async () => {
      try {
        const configData = await apiClient.getConfig();
        useChatStore.setState({
          thinkingLevel: configData.thinking_level || 'Medium',
        });
        useChatStore.getState().setStatus({
          mode: configData.mode || 'normal',
          autonomy_level: configData.autonomy_level || 'Manual',
          thinking_level: configData.thinking_level || 'Medium',
          model: configData.model,
          model_provider: configData.model_provider,
          working_dir: configData.working_dir || '',
          git_branch: configData.git_branch,
        });
      } catch (_) { /* ignore */ }
    };
    loadStatus();
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key === 'T') {
        e.preventDefault();
        cycleThinkingLevel();
      }
      if (e.ctrlKey && e.shiftKey && e.key === 'A') {
        e.preventDefault();
        cycleAutonomy();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
        e.preventDefault();
        toggleSidebar();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [cycleThinkingLevel, cycleAutonomy, toggleSidebar]);

  const getProjectName = (path: string) => {
    if (!path) return '';
    const parts = path.replace(/\/$/, '').split('/');
    return parts[parts.length - 1] || path;
  };

  const pillBase = 'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full border text-xs font-medium cursor-pointer transition-colors select-none';

  return (
    <header className="h-12 flex-shrink-0 sticky top-0 z-40 flex items-center gap-3 px-4 bg-bg-000 border-b border-beige-200">
      {/* ── Left: Sidebar toggle + Brand ── */}
      <div className="flex items-center gap-3 flex-shrink-0">
        <button
          onClick={toggleSidebar}
          className="w-8 h-8 rounded-md flex items-center justify-center hover:bg-beige-200/50 transition-colors"
          title={sidebarCollapsed ? 'Expand sidebar (Ctrl/Cmd+B)' : 'Collapse sidebar (Ctrl/Cmd+B)'}
        >
          <PanelLeft className="w-5 h-5 text-beige-600" />
        </button>

        {/* Logo */}
        <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-amber-400 via-orange-500 to-rose-500 flex items-center justify-center shadow-sm flex-shrink-0">
          <svg className="w-4 h-4 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
          </svg>
        </div>

        <div className="flex items-baseline gap-1.5">
          <span className="text-sm font-bold tracking-tight text-gray-900">OPENDEV</span>
          <span className="text-[10px] uppercase tracking-wider text-beige-500 hidden sm:inline">AI Assistant</span>
        </div>
      </div>

      {/* ── Spacer ── */}
      <div className="flex-1" />

      {/* ── Center-Right: Status Pills ── */}
      {status && (
        <div className="flex items-center gap-2 flex-shrink-0">
          {/* Mode pill */}
          <button
            onClick={toggleMode}
            className={`${pillBase} ${MODE_STYLES[status.mode]}`}
            title="Normal: full tool access · Plan: read-only exploration. Click to toggle"
          >
            {status.mode === 'plan' && (
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
              </svg>
            )}
            Mode: {status.mode === 'normal' ? 'Normal' : 'Plan'}
          </button>

          {/* Autonomy pill */}
          <button
            onClick={cycleAutonomy}
            className={`${pillBase} ${AUTONOMY_STYLES[status.autonomy_level]}`}
            title="Manual: approve each tool · Semi-Auto: auto-approve safe tools · Auto: approve all. Click to cycle (Ctrl+Shift+A)"
          >
            Approval: {status.autonomy_level}
          </button>

          {/* Thinking pill */}
          <button
            onClick={cycleThinkingLevel}
            className={`${pillBase} ${THINKING_STYLES[thinkingLevel] || THINKING_STYLES['Medium']}`}
            title="Controls how much the AI reasons before responding. Click to cycle (Ctrl+Shift+T)"
          >
            Think: {thinkingLevel}
          </button>

          {/* Connection pill */}
          <span className={`${pillBase} cursor-default ${
            isConnected
              ? 'bg-emerald-50 text-emerald-700 border-emerald-200'
              : 'bg-gray-100 text-gray-500 border-gray-200'
          }`}>
            <span className={`w-2 h-2 rounded-full ${isConnected ? 'bg-emerald-500' : 'bg-gray-400'}`} />
            {isConnected ? 'Connected' : 'Offline'}
          </span>
        </div>
      )}

      {/* ── Far-Right: Project / Model ── */}
      {status && (
        <div className="flex items-center gap-2 text-xs text-text-500 flex-shrink-0 ml-1 hidden md:flex">
          {status.working_dir && (
            <span className="truncate max-w-[160px]" title={status.working_dir}>
              {getProjectName(status.working_dir)}
              {status.git_branch && (
                <span className="text-text-400">
                  <span className="text-text-500"> / </span>{status.git_branch}
                </span>
              )}
            </span>
          )}

          {status.working_dir && status.model && (
            <span className="text-beige-300">|</span>
          )}

          {status.model && (
            <span className="font-mono text-text-400 truncate max-w-[140px]" title={`${status.model_provider}/${status.model}`}>
              {status.model}
            </span>
          )}
        </div>
      )}
    </header>
  );
}
