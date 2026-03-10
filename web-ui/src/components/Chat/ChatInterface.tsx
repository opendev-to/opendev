import { useEffect, useState } from 'react';
import { useChatStore } from '../../stores/chat';
import { apiClient } from '../../api/client';
import { MessageList } from './MessageList';
import { QueueBar } from './QueueBar';
import { InputBox } from './InputBox';
import { LandingPage } from './LandingPage';

export function ChatInterface() {
  const error = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.error ?? null : null;
  });
  const currentSessionId = useChatStore(state => state.currentSessionId);
  const loadSession = useChatStore(state => state.loadSession);
  const [bridgeChecked, setBridgeChecked] = useState(false);

  // Auto-join TUI session in bridge mode
  useEffect(() => {
    let cancelled = false;
    apiClient.getBridgeInfo().then(info => {
      if (cancelled) return;
      if (info.bridge_mode && info.session_id) {
        loadSession(info.session_id);
      }
      setBridgeChecked(true);
    }).catch(() => {
      if (!cancelled) setBridgeChecked(true);
    });
    return () => { cancelled = true; };
  }, [loadSession]);

  // Brief null render while checking bridge info (imperceptible)
  if (!bridgeChecked && !currentSessionId) {
    return null;
  }

  if (!currentSessionId) {
    return <LandingPage />;
  }

  return (
    <div className="flex flex-col h-full relative animate-fade-in">
      {error && (
        <div className="bg-red-50 border border-red-200 text-red-800 px-4 py-3 mx-6 mt-4 rounded-lg">
          <strong className="font-semibold">Error:</strong> {error}
        </div>
      )}

      <MessageList />
      <QueueBar />
      <InputBox />
    </div>
  );
}
