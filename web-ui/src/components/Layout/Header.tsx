import { useChatStore } from '../../stores/chat';

export function Header() {
  const isConnected = useChatStore(state => state.isConnected);

  return (
    <header className="bg-white border-b border-gray-200 px-6 py-4">
      <div className="flex items-center justify-between max-w-4.5xl mx-auto">
        <div className="flex items-center gap-3">
          <h1 className="text-lg font-semibold text-gray-900">OpenDev</h1>
          <span className="text-sm text-gray-500">Web Interface</span>
        </div>

        <div className="flex items-center gap-2">
          <div className={`w-1.5 h-1.5 rounded-full ${isConnected ? 'bg-green-500' : 'bg-gray-300'}`} />
          <span className="text-xs text-gray-600">
            {isConnected ? 'Connected' : 'Disconnected'}
          </span>
        </div>
      </div>
    </header>
  );
}
