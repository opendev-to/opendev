import { useState, useEffect, useRef, KeyboardEvent } from 'react';
import { useChatStore } from '../../stores/chat';
import { apiClient } from '../../api/client';
import { SPINNER_FRAMES } from '../../constants/spinner';
import { NewSessionModal } from '../Layout/NewSessionModal';

interface WorkspaceOption {
  path: string;
  projectName: string;
}

const getProjectName = (path: string): string => {
  const parts = path.replace(/\/$/, '').split('/');
  return parts[parts.length - 1] || path;
};

export function LandingPage() {
  const [input, setInput] = useState('');
  const [workspaces, setWorkspaces] = useState<WorkspaceOption[]>([]);
  const [selectedWorkspace, setSelectedWorkspace] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showPlusMenu, setShowPlusMenu] = useState(false);
  const [showWorkspacePicker, setShowWorkspacePicker] = useState(false);
  const [attachedFiles, setAttachedFiles] = useState<File[]>([]);
  const [isNewSessionOpen, setIsNewSessionOpen] = useState(false);
  const [brailleOffset, setBrailleOffset] = useState(0);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const plusMenuRef = useRef<HTMLDivElement>(null);
  const workspaceMenuRef = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const fileAcceptRef = useRef<string>('');

  const isConnected = useChatStore(state => state.isConnected);
  const loadSession = useChatStore(state => state.loadSession);
  const sendMessage = useChatStore(state => state.sendMessage);
  const bumpSessionList = useChatStore(state => state.bumpSessionList);

  // Fetch workspaces on mount
  useEffect(() => {
    apiClient.listSessions().then(sessions => {
      // Group by working_dir, sort by most recent
      const grouped: Record<string, string> = {};
      const recency: Record<string, number> = {};

      for (const s of sessions) {
        if (!s.working_dir || !s.working_dir.trim()) continue;
        const t = new Date(s.updated_at).getTime();
        if (!recency[s.working_dir] || t > recency[s.working_dir]) {
          recency[s.working_dir] = t;
        }
        grouped[s.working_dir] = s.working_dir;
      }

      const sorted = Object.keys(grouped)
        .sort((a, b) => (recency[b] || 0) - (recency[a] || 0))
        .map(path => ({ path, projectName: getProjectName(path) }));

      setWorkspaces(sorted);
      if (sorted.length > 0) {
        setSelectedWorkspace(sorted[0].path);
      }
    }).catch(console.error);
  }, []);

  // Braille halo animation
  useEffect(() => {
    const interval = setInterval(() => {
      setBrailleOffset(prev => (prev + 1) % SPINNER_FRAMES.length);
    }, 100);
    return () => clearInterval(interval);
  }, []);

  // Click-outside to dismiss menus
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (plusMenuRef.current && !plusMenuRef.current.contains(e.target as Node)) {
        setShowPlusMenu(false);
      }
      if (workspaceMenuRef.current && !workspaceMenuRef.current.contains(e.target as Node)) {
        setShowWorkspacePicker(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 200) + 'px';
    }
  }, [input]);

  const handleSend = async () => {
    if (!input.trim() || isCreating || !isConnected) return;
    if (!selectedWorkspace) {
      setError('Select a workspace first');
      return;
    }

    setIsCreating(true);
    setError(null);

    try {
      const result = await apiClient.createSession(selectedWorkspace);
      bumpSessionList();
      await loadSession(result.session.id);
      sendMessage(input.trim());
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create session');
      setIsCreating(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleFileUpload = (accept: string) => {
    fileAcceptRef.current = accept;
    setShowPlusMenu(false);
    // Trigger after state update
    setTimeout(() => {
      if (fileInputRef.current) {
        fileInputRef.current.accept = accept;
        fileInputRef.current.click();
      }
    }, 0);
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files) {
      setAttachedFiles(prev => [...prev, ...Array.from(files)]);
    }
    // Reset input so the same file can be re-selected
    e.target.value = '';
  };

  const removeFile = (index: number) => {
    setAttachedFiles(prev => prev.filter((_, i) => i !== index));
  };

  const selectedProject = workspaces.find(w => w.path === selectedWorkspace);

  return (
    <div className="relative flex flex-col items-center justify-center h-full px-6 bg-bg-100 overflow-hidden">
      {/* Background watermark layer */}
      <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
        <span className="text-5xl md:text-7xl font-mono font-bold tracking-wider text-bg-300 animate-breathe select-none">
          OpenDev
        </span>
        <div className="absolute animate-spin-slow" style={{ width: 360, height: 360 }}>
          {Array.from({ length: 24 }).map((_, i) => {
            const angle = (i / 24) * 360;
            const char = SPINNER_FRAMES[(i + brailleOffset) % SPINNER_FRAMES.length];
            return (
              <span
                key={i}
                className="absolute text-lg font-mono text-bg-300"
                style={{
                  left: '50%',
                  top: '50%',
                  transform: `rotate(${angle}deg) translateX(180px) rotate(-${angle}deg)`,
                }}
              >
                {char}
              </span>
            );
          })}
        </div>
      </div>

      {/* Centered input card */}
      <div className="relative z-10 w-full max-w-2xl animate-fade-in">
        <h2 className="text-2xl font-semibold text-text-000 mb-6 text-center">
          What are you working on?
        </h2>
        <div className="rounded-2xl border border-border-300/20 bg-bg-000 shadow-lg">
          {/* Textarea area */}
          <div className="px-5 pt-5 pb-2 rounded-t-2xl">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="How can I help you today?"
              disabled={isCreating || !isConnected}
              className="w-full bg-transparent text-text-000 placeholder-text-400 resize-none border-0 focus:outline-none focus:ring-0 text-base leading-relaxed disabled:opacity-50 disabled:cursor-not-allowed"
              rows={3}
              style={{ minHeight: '80px' }}
            />

            {/* Attached file chips */}
            {attachedFiles.length > 0 && (
              <div className="flex flex-wrap gap-2 mt-2">
                {attachedFiles.map((file, i) => (
                  <span
                    key={i}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-bg-200 text-text-200 text-xs border border-border-300/15"
                  >
                    <svg className="w-3.5 h-3.5 text-text-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
                    </svg>
                    {file.name}
                    <button
                      onClick={() => removeFile(i)}
                      className="ml-0.5 text-text-400 hover:text-danger-100"
                    >
                      <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Bottom utility bar */}
          <div className="flex items-center justify-between px-4 py-3 border-t border-border-300/10 rounded-b-2xl">
            {/* Left: + button */}
            <div className="relative" ref={plusMenuRef}>
              <button
                onClick={() => setShowPlusMenu(!showPlusMenu)}
                className="w-8 h-8 rounded-full flex items-center justify-center bg-bg-200 hover:bg-bg-300 text-text-300 hover:text-text-100 transition-colors"
                title="Attach files"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
                </svg>
              </button>

              {/* Plus menu popover */}
              {showPlusMenu && (
                <div className="absolute bottom-full left-0 mb-2 w-48 bg-bg-000 border border-border-300/20 rounded-xl shadow-lg overflow-hidden z-50 animate-fade-in">
                  <button
                    onClick={() => handleFileUpload('.png,.jpg,.jpeg,.gif,.webp')}
                    className="w-full px-4 py-2.5 text-left text-sm text-text-100 hover:bg-bg-200 flex items-center gap-2.5"
                  >
                    <svg className="w-4 h-4 text-text-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                    </svg>
                    Upload image
                  </button>
                  <button
                    onClick={() => handleFileUpload('.pdf,.docx')}
                    className="w-full px-4 py-2.5 text-left text-sm text-text-100 hover:bg-bg-200 flex items-center gap-2.5"
                  >
                    <svg className="w-4 h-4 text-text-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                    </svg>
                    Upload document
                  </button>
                </div>
              )}
            </div>

            {/* Center-right: Workspace badge */}
            <div className="flex items-center gap-2">
              {workspaces.length > 0 && (
                <div className="relative" ref={workspaceMenuRef}>
                  <button
                    onClick={() => setShowWorkspacePicker(!showWorkspacePicker)}
                    className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-text-300 hover:text-text-100 bg-bg-200 hover:bg-bg-300 transition-colors"
                  >
                    <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                    {selectedProject?.projectName || 'Select workspace'}
                    <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                    </svg>
                  </button>

                  {/* Workspace dropdown */}
                  {showWorkspacePicker && (
                    <div className="absolute bottom-full right-0 mb-2 w-64 bg-bg-000 border border-border-300/20 rounded-xl shadow-lg overflow-hidden z-50 animate-fade-in">
                      <div className="max-h-48 overflow-y-auto py-1">
                        {workspaces.map(w => (
                          <button
                            key={w.path}
                            onClick={() => {
                              setSelectedWorkspace(w.path);
                              setShowWorkspacePicker(false);
                            }}
                            className={`w-full px-4 py-2.5 text-left text-sm hover:bg-bg-200 flex flex-col ${
                              w.path === selectedWorkspace ? 'bg-bg-200' : ''
                            }`}
                            title={w.path}
                          >
                            <span className="text-text-100 font-medium">{w.projectName}</span>
                            <span className="text-text-400 text-xs truncate">{w.path}</span>
                          </button>
                        ))}
                      </div>
                      <div className="border-t border-border-300/10">
                        <button
                          onClick={() => {
                            setShowWorkspacePicker(false);
                            setIsNewSessionOpen(true);
                          }}
                          className="w-full px-4 py-2.5 text-left text-sm text-accent-main-100 hover:bg-bg-200 flex items-center gap-2"
                        >
                          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                          </svg>
                          New workspace...
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* Send button */}
              <button
                onClick={handleSend}
                disabled={!input.trim() || isCreating || !isConnected}
                className="w-8 h-8 rounded-lg flex items-center justify-center bg-accent-main-100 hover:bg-accent-main-200 text-white disabled:opacity-40 disabled:cursor-not-allowed disabled:bg-bg-300 disabled:text-text-500 transition-colors"
                title="Send (Enter)"
              >
                {isCreating ? (
                  <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                  </svg>
                ) : (
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
                  </svg>
                )}
              </button>
            </div>
          </div>
        </div>

        {/* Error message */}
        {error && (
          <p className="mt-3 text-sm text-danger-100 text-center animate-fade-in">{error}</p>
        )}

        {/* Hint text */}
        <p className="mt-4 text-xs text-text-400 text-center">
          <kbd className="px-1.5 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs">Enter</kbd> to send
          {' '}&middot;{' '}
          <kbd className="px-1.5 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs">Shift + Enter</kbd> for new line
          {' '}&middot; auto-creates a session
        </p>
      </div>

      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        className="hidden"
        onChange={handleFileChange}
      />

      {/* New workspace modal */}
      <NewSessionModal
        isOpen={isNewSessionOpen}
        onClose={() => setIsNewSessionOpen(false)}
      />
    </div>
  );
}
