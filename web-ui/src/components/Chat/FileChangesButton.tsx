import { useState } from 'react';
import { useFileChangesStore } from '../../stores/fileChanges';
import { useChatStore } from '../../stores/chat';

export function FileChangesButton() {
  const [isModalOpen, setIsModalOpen] = useState(false);
  const { changes, summary, loadFileChanges, isLoading } = useFileChangesStore();
  const { currentSessionId } = useChatStore(state => state);

  const handleClick = () => {
    if (currentSessionId && !isModalOpen) {
      loadFileChanges(currentSessionId);
    }
    setIsModalOpen(!isModalOpen);
  };

  const hasChanges = changes && changes.length > 0;
  const changeCount = changes.length;

  return (
    <>
      <button
        onClick={handleClick}
        className="flex items-center gap-2 px-3 py-2 text-xs text-gray-600 bg-gray-50 border border-gray-200 rounded-lg hover:bg-gray-100 hover:text-gray-800 transition-colors"
        title={hasChanges ? `${changeCount} file changes` : 'View file changes'}
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
        </svg>
        <span className="font-medium">File Changes</span>
        {hasChanges && (
          <span className="px-1.5 py-0.5 bg-blue-100 text-blue-700 rounded-md font-medium">
            {changeCount}
          </span>
        )}
      </button>

      {isModalOpen && (
        <FileChangesModal
          onClose={() => setIsModalOpen(false)}
          changes={changes}
          summary={summary}
          isLoading={isLoading}
        />
      )}
    </>
  );
}

interface FileChangesModalProps {
  onClose: () => void;
  changes: any[];
  summary: any;
  isLoading: boolean;
}

function FileChangesModal({ onClose, changes, summary, isLoading }: FileChangesModalProps) {
  return (
    <div className="fixed inset-0 z-50 overflow-y-auto bg-black bg-opacity-50 flex items-center justify-center p-4">
      <div className="bg-white rounded-lg shadow-xl max-w-4xl w-full max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200">
          <h2 className="text-lg font-semibold text-gray-900 flex items-center gap-2">
            <svg className="w-5 h-5 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            File Changes
          </h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 transition-colors"
          >
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Summary */}
        {summary && (
          <div className="p-4 bg-gray-50 border-b border-gray-200">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-center">
              <div className="bg-white p-3 rounded-lg border border-gray-200">
                <div className="text-xl font-bold text-blue-600">{summary.total}</div>
                <div className="text-xs text-gray-600">Total Changes</div>
              </div>
              <div className="bg-white p-3 rounded-lg border border-gray-200">
                <div className="text-xl font-bold text-green-600">+{summary.total_lines_added}</div>
                <div className="text-xs text-gray-600">Lines Added</div>
              </div>
              <div className="bg-white p-3 rounded-lg border border-gray-200">
                <div className="text-xl font-bold text-red-600">-{summary.total_lines_removed}</div>
                <div className="text-xs text-gray-600">Lines Removed</div>
              </div>
              <div className="bg-white p-3 rounded-lg border border-gray-200">
                <div className="text-xl font-bold text-purple-600">{summary.net_lines}</div>
                <div className="text-xs text-gray-600">Net Change</div>
              </div>
            </div>
          </div>
        )}

        {/* Changes List */}
        <div className="flex-1 overflow-y-auto p-4">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          ) : changes && changes.length > 0 ? (
            <div className="space-y-2">
              {changes.map((change, index) => (
                <FileChangeItem key={index} change={change} />
              ))}
            </div>
          ) : (
            <div className="text-center py-8 text-gray-500">
              <svg className="w-12 h-12 mx-auto mb-4 text-gray-300" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
              <div className="text-lg font-medium">No file changes yet</div>
              <div className="text-sm">Start making changes to see them here</div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-4 border-t border-gray-200 bg-gray-50 rounded-b-lg">
          <div className="flex justify-between items-center">
            <div className="text-xs text-gray-500">
              {changes.length} change{changes.length !== 1 ? 's' : ''} tracked
            </div>
            <button
              onClick={onClose}
              className="px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-lg hover:bg-blue-700 transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

interface FileChangeItemProps {
  change: any;
}

function FileChangeItem({ change }: FileChangeItemProps) {
  const getTimeAgo = (timestamp: string) => {
    const now = new Date();
    const changeTime = new Date(timestamp);
    const diffInMinutes = Math.floor((now.getTime() - changeTime.getTime()) / (1000 * 60));

    if (diffInMinutes < 1) return 'just now';
    if (diffInMinutes < 60) return `${diffInMinutes}m ago`;
    if (diffInMinutes < 1440) return `${Math.floor(diffInMinutes / 60)}h ago`;
    return `${Math.floor(diffInMinutes / 1440)}d ago`;
  };

  const getStatusColor = (color: string) => {
    switch (color) {
      case 'green': return 'text-green-600 bg-green-50 border-green-200';
      case 'blue': return 'text-blue-600 bg-blue-50 border-blue-200';
      case 'red': return 'text-red-600 bg-red-50 border-red-200';
      case 'orange': return 'text-orange-600 bg-orange-50 border-orange-200';
      default: return 'text-gray-600 bg-gray-50 border-gray-200';
    }
  };

  const statusClasses = getStatusColor(change.color);

  return (
    <div className="bg-white border border-gray-200 rounded-lg p-3 hover:bg-gray-50 transition-colors">
      <div className="flex items-center gap-3">
        <span className="text-lg">{change.icon}</span>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <span className="font-medium text-gray-900 truncate">
              {change.file_path.split('/').pop()}
            </span>
            <span className={`px-2 py-1 rounded text-xs font-medium border ${statusClasses}`}>
              {change.type}
            </span>
          </div>

          <div className="text-xs text-gray-500 mb-1">
            {change.file_path}
          </div>

          {change.summary && (
            <div className="text-xs text-gray-600 font-mono mb-1">
              {change.summary}
            </div>
          )}

          {change.description && (
            <div className="text-xs text-gray-500">
              {change.description}
            </div>
          )}
        </div>

        <div className="text-right">
          <div className="text-xs text-gray-500">
            {getTimeAgo(change.timestamp)}
          </div>
          {(change.lines_added > 0 || change.lines_removed > 0) && (
            <div className="text-xs font-medium mt-1">
              <span className="text-green-600">+{change.lines_added}</span>
              <span className="text-red-600 ml-1">-{change.lines_removed}</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}