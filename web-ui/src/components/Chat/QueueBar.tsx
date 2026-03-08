import { useState } from 'react';
import { useChatStore } from '../../stores/chat';

export function QueueBar() {
  const [expanded, setExpanded] = useState(false);

  const queuedMessages = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.queuedMessages ?? [] : [];
  });

  if (queuedMessages.length === 0) return null;

  return (
    <div className="px-4">
      <div className="border-t border-border-300/15 bg-bg-200/50 rounded-t-lg overflow-hidden">
        <button
          onClick={() => setExpanded(prev => !prev)}
          className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-text-300 hover:text-text-100 transition-colors"
        >
          <svg
            className={`w-3 h-3 transition-transform ${expanded ? 'rotate-90' : ''}`}
            fill="currentColor"
            viewBox="0 0 20 20"
          >
            <path
              fillRule="evenodd"
              d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z"
              clipRule="evenodd"
            />
          </svg>
          <span>
            {queuedMessages.length} message{queuedMessages.length !== 1 ? 's' : ''} queued
          </span>
        </button>

        <div
          className="transition-all duration-200 ease-in-out overflow-hidden"
          style={{ maxHeight: expanded ? `${queuedMessages.length * 40 + 8}px` : '0px' }}
        >
          <div className="px-3 pb-2 space-y-1">
            {queuedMessages.map((msg, i) => (
              <div
                key={i}
                className="text-xs text-text-300 bg-bg-100/60 rounded px-2.5 py-1.5 truncate"
                title={msg}
              >
                {msg}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
