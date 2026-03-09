import { useEffect, useRef, useState, useCallback } from 'react';
import { useChatStore } from '../../stores/chat';

interface Command {
  id: string;
  label: string;
  description: string;
  action: () => void;
}

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenStatus: () => void;
}

export function CommandPalette({ isOpen, onClose, onOpenStatus }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const toggleMode = useChatStore(state => state.toggleMode);
  const cycleAutonomy = useChatStore(state => state.cycleAutonomy);
  const cycleThinkingLevel = useChatStore(state => state.cycleThinkingLevel);
  const clearChat = useChatStore(state => state.clearChat);
  const sendInterrupt = useChatStore(state => state.sendInterrupt);
  const toggleSidebar = useChatStore(state => state.toggleSidebar);

  const commands: Command[] = [
    { id: 'clear', label: '/clear', description: 'Clear chat history', action: () => { clearChat(); onClose(); } },
    { id: 'mode', label: '/mode', description: 'Toggle between Normal and Plan mode', action: () => { toggleMode(); onClose(); } },
    { id: 'status', label: '/status', description: 'Show session status dialog', action: () => { onOpenStatus(); onClose(); } },
    { id: 'interrupt', label: '/interrupt', description: 'Interrupt the current task', action: () => { sendInterrupt(); onClose(); } },
    { id: 'autonomy', label: 'Cycle Autonomy', description: 'Cycle: Manual → Semi-Auto → Auto', action: () => { cycleAutonomy(); onClose(); } },
    { id: 'thinking', label: 'Cycle Thinking', description: 'Cycle: Off → Low → Medium → High', action: () => { cycleThinkingLevel(); onClose(); } },
    { id: 'sidebar', label: 'Toggle Sidebar', description: 'Show/hide the sessions sidebar', action: () => { toggleSidebar(); onClose(); } },
  ];

  const filtered = query
    ? commands.filter(c =>
        c.label.toLowerCase().includes(query.toLowerCase()) ||
        c.description.toLowerCase().includes(query.toLowerCase())
      )
    : commands;

  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex(prev => Math.min(prev + 1, filtered.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex(prev => Math.max(prev - 1, 0));
    } else if (e.key === 'Enter' && filtered.length > 0) {
      e.preventDefault();
      filtered[selectedIndex]?.action();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }, [filtered, selectedIndex, onClose]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[20vh]" onClick={onClose}>
      <div className="absolute inset-0 bg-black/30" />
      <div
        className="relative w-full max-w-lg bg-bg-000 border border-border-300/30 rounded-xl shadow-2xl overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={e => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a command..."
          className="w-full px-4 py-3 text-sm bg-transparent border-b border-border-300/20 text-text-000 placeholder-text-400 outline-none"
        />
        <div className="max-h-64 overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-3 text-sm text-text-400">No matching commands</div>
          ) : (
            filtered.map((cmd, i) => (
              <button
                key={cmd.id}
                onClick={cmd.action}
                className={`w-full text-left px-4 py-2.5 flex items-center gap-3 text-sm transition-colors ${
                  i === selectedIndex ? 'bg-accent-main-100/10 text-text-000' : 'text-text-200 hover:bg-bg-200'
                }`}
              >
                <span className="font-mono font-medium text-accent-main-100 min-w-[120px]">{cmd.label}</span>
                <span className="text-text-300">{cmd.description}</span>
              </button>
            ))
          )}
        </div>
        <div className="px-4 py-2 border-t border-border-300/20 text-xs text-text-400 flex gap-3">
          <span><kbd className="px-1 py-0.5 bg-bg-200 rounded text-text-300">↑↓</kbd> navigate</span>
          <span><kbd className="px-1 py-0.5 bg-bg-200 rounded text-text-300">Enter</kbd> select</span>
          <span><kbd className="px-1 py-0.5 bg-bg-200 rounded text-text-300">Esc</kbd> close</span>
        </div>
      </div>
    </div>
  );
}
