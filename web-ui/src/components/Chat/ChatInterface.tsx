import { useChatStore } from '../../stores/chat';
import { MessageList } from './MessageList';
import { InputBox } from './InputBox';
import { LandingPage } from './LandingPage';

export function ChatInterface() {
  const error = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.error ?? null : null;
  });
  const currentSessionId = useChatStore(state => state.currentSessionId);

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
      <InputBox />
    </div>
  );
}
