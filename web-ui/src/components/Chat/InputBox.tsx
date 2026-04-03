import { useState, KeyboardEvent, useRef, useEffect } from 'react';
import { useChatStore } from '../../stores/chat';
import { apiClient } from '../../api/client';
import { FileMentionDropdown } from './FileMentionDropdown';
import { FileChangesButton } from './FileChangesButton';

interface FileItem {
  path: string;
  name: string;
  is_file: boolean;
}

export function InputBox() {
  const [input, setInput] = useState('');
  const [showFileMention, setShowFileMention] = useState(false);
  const [filesList, setFilesList] = useState<FileItem[]>([]);
  const [selectedFileIndex, setSelectedFileIndex] = useState(0);
  const [mentionPosition, setMentionPosition] = useState({ top: 0, left: 0 });
  const [mentionQuery, setMentionQuery] = useState('');
  const [mentionStartPos, setMentionStartPos] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const sendMessage = useChatStore(state => state.sendMessage);
  const isLoading = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.isLoading ?? false : false;
  });
  const isConnected = useChatStore(state => state.isConnected);
  const currentSessionId = useChatStore(state => state.currentSessionId);
  const hasActiveSession = !!currentSessionId;

  // Load files when @ is detected (with debounce)
  useEffect(() => {
    if (!showFileMention) {
      return;
    }

    const timer = setTimeout(() => {
      apiClient.listFiles(mentionQuery).then(response => {
        setFilesList(response.files);
        setSelectedFileIndex(0);
      }).catch(error => {
        console.error('Failed to load files:', error);
        setFilesList([]);
      });
    }, 300); // 300ms debounce

    return () => clearTimeout(timer);
  }, [mentionQuery, showFileMention]);

  const handleSend = () => {
    if (!input.trim() || !isConnected || !hasActiveSession) return;

    sendMessage(input.trim());
    setInput('');
    setShowFileMention(false);
  };

  const handleStop = async () => {
    try {
      await apiClient.interruptTask();
    } catch (error) {
      console.error('Failed to interrupt task:', error);
    }
  };

  const handleFileSelect = (file: FileItem) => {
    if (!textareaRef.current) return;

    // Replace @query with @file.path
    const before = input.substring(0, mentionStartPos);
    const after = input.substring(textareaRef.current.selectionStart);
    const newInput = before + '@' + file.path + ' ' + after;

    setInput(newInput);
    setShowFileMention(false);

    // Set cursor position after the inserted file path
    setTimeout(() => {
      if (textareaRef.current) {
        const newPos = mentionStartPos + file.path.length + 2; // +2 for @ and space
        textareaRef.current.setSelectionRange(newPos, newPos);
        textareaRef.current.focus();
      }
    }, 0);
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newInput = e.target.value;
    const cursorPos = e.target.selectionStart;

    setInput(newInput);

    // Check if @ was just typed or is in the current word
    const textBeforeCursor = newInput.substring(0, cursorPos);
    const lastAtIndex = textBeforeCursor.lastIndexOf('@');

    if (lastAtIndex !== -1) {
      // Check if there's a space between @ and cursor (which would end the mention)
      const textAfterAt = textBeforeCursor.substring(lastAtIndex + 1);

      if (!textAfterAt.includes(' ') && textAfterAt.length >= 0) {
        // We're in a mention
        setMentionStartPos(lastAtIndex);
        setMentionQuery(textAfterAt);
        setShowFileMention(true);

        // Calculate dropdown position relative to viewport (show above input box)
        if (textareaRef.current) {
          const rect = textareaRef.current.getBoundingClientRect();
          // Position dropdown above the textarea, accounting for dropdown height
          setMentionPosition({
            top: rect.top - 270, // 270px = max-h-64 (256px) + some margin
            left: rect.left + 20
          });
        }
      } else {
        setShowFileMention(false);
      }
    } else {
      setShowFileMention(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Handle file mention dropdown navigation
    if (showFileMention && filesList.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedFileIndex((prev) => (prev + 1) % filesList.length);
        return;
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedFileIndex((prev) => (prev - 1 + filesList.length) % filesList.length);
        return;
      } else if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleFileSelect(filesList[selectedFileIndex]);
        return;
      } else if (e.key === 'Escape') {
        e.preventDefault();
        setShowFileMention(false);
        return;
      }
    }

    // Normal keyboard handling
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    } else if (e.key === 'Escape' && isLoading) {
      e.preventDefault();
      handleStop();
    }
  };

  return (
    <div className="bg-bg-000 p-4">
      <div className="w-full relative">
        <div className="rounded-lg border-0.5 border-border-300/15 bg-bg-000/60 focus-within:bg-bg-000 focus-within:shadow-sm transition-all">
          <div className="flex gap-2 p-2">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              placeholder={
                !hasActiveSession
                  ? "Select a session to start chatting..."
                  : !isConnected
                  ? "Disconnected..."
                  : isLoading
                  ? "Type to queue a message..."
                  : "Type your message... (use @ to mention files)"
              }
              disabled={!isConnected || !hasActiveSession}
              className="flex-1 bg-transparent text-text-000 placeholder-text-500 rounded-md px-3 py-2 resize-none border-0 focus:outline-none focus:ring-0 disabled:opacity-50 disabled:cursor-not-allowed"
              rows={2}
            />
            <div className="flex gap-1.5 self-end">
              {isLoading && (
                <button
                  onClick={handleStop}
                  className="px-3 py-2 rounded-lg transition-colors font-medium bg-danger-100 hover:bg-danger-000 text-white hover-scale"
                  title="Stop (Esc)"
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                    <rect x="6" y="6" width="12" height="12" rx="1" />
                  </svg>
                </button>
              )}
              <button
                onClick={handleSend}
                disabled={!input.trim() || !isConnected || !hasActiveSession}
                className="px-4 py-2 rounded-lg transition-colors font-medium bg-accent-main-100 hover:bg-accent-main-200 text-white disabled:opacity-40 disabled:cursor-not-allowed disabled:bg-bg-300 disabled:text-text-500 hover-scale"
                title="Send (Enter)"
              >
                <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
                </svg>
              </button>
            </div>
          </div>
        </div>

        <div className="flex items-center justify-between mt-2">
          <div className="text-xs text-text-500 px-1">
            Press <kbd className="px-1.5 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs">@</kbd> to mention files · <kbd className="px-1.5 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs">Enter</kbd> to send · <kbd className="px-1.5 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs">Shift + Enter</kbd> for new line · <kbd className="px-1.5 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs">Esc</kbd> to stop
          </div>

          {hasActiveSession && (
            <FileChangesButton />
          )}
        </div>

        {showFileMention && (
          <FileMentionDropdown
            files={filesList}
            selectedIndex={selectedFileIndex}
            onSelect={handleFileSelect}
            onClose={() => setShowFileMention(false)}
            position={mentionPosition}
          />
        )}
      </div>
    </div>
  );
}
