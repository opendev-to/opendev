import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { XMarkIcon, ChevronRightIcon, FolderIcon } from '@heroicons/react/24/outline';
import { apiClient } from '../../api/client';

interface NewSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface DirEntry {
  name: string;
  path: string;
}

const getBreadcrumbs = (absPath: string) => {
  const parts = absPath.split('/').filter(Boolean);
  return parts.map((part, i) => ({
    label: part,
    path: '/' + parts.slice(0, i + 1).join('/'),
  }));
};

export function NewSessionModal({ isOpen, onClose }: NewSessionModalProps) {
  const [currentPath, setCurrentPath] = useState('');
  const [parentPath, setParentPath] = useState<string | null>(null);
  const [directories, setDirectories] = useState<DirEntry[]>([]);
  const [manualPath, setManualPath] = useState('');
  const [showHidden, setShowHidden] = useState(false);
  const [isLoadingDirs, setIsLoadingDirs] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  const fetchDirectory = async (path: string, hidden?: boolean) => {
    setIsLoadingDirs(true);
    setBrowseError(null);
    try {
      const result = await apiClient.browseDirectory(path, hidden ?? showHidden);
      setCurrentPath(result.current_path);
      setParentPath(result.parent_path);
      setDirectories(result.directories);
      setManualPath(result.current_path);
      if (result.error) {
        setBrowseError(result.error);
      }
    } catch (err) {
      setBrowseError('Failed to browse directory');
    } finally {
      setIsLoadingDirs(false);
    }
  };

  // Load home directory on open
  useEffect(() => {
    if (isOpen) {
      fetchDirectory('');
    }
  }, [isOpen]);

  // Refetch when showHidden toggles
  useEffect(() => {
    if (isOpen && currentPath) {
      fetchDirectory(currentPath, showHidden);
    }
  }, [showHidden]);

  const handleManualGo = () => {
    if (manualPath.trim()) {
      fetchDirectory(manualPath.trim());
    }
  };

  const handleSelect = async () => {
    if (!currentPath) return;
    setIsCreating(true);
    try {
      await apiClient.createSession(currentPath);
      window.location.reload();
    } catch (err) {
      console.error('Failed to create session:', err);
      setIsCreating(false);
    }
  };

  if (!isOpen) return null;

  const breadcrumbs = currentPath ? getBreadcrumbs(currentPath) : [];

  const modalContent = (
    <div
      className="fixed inset-0 bg-black/60 z-[99999] flex items-center justify-center"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-xl shadow-2xl max-w-lg w-full max-h-[80vh] flex flex-col mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
          <h2 className="text-lg font-semibold text-gray-900">Select Workspace</h2>
          <button
            onClick={onClose}
            className="p-1 rounded-md hover:bg-gray-100 text-gray-400 hover:text-gray-600"
          >
            <XMarkIcon className="w-5 h-5" />
          </button>
        </div>

        {/* Path input bar */}
        <div className="px-5 pt-4 pb-2">
          <div className="flex gap-2">
            <input
              type="text"
              value={manualPath}
              onChange={(e) => setManualPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleManualGo();
              }}
              placeholder="/path/to/directory"
              className="flex-1 px-3 py-2 text-sm font-mono border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-amber-400 focus:border-transparent"
            />
            <button
              onClick={handleManualGo}
              disabled={!manualPath.trim()}
              className="px-4 py-2 text-sm font-medium bg-gray-100 hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed rounded-lg text-gray-700 border border-gray-300"
            >
              Go
            </button>
          </div>
        </div>

        {/* Breadcrumb bar */}
        <div className="px-5 py-2 flex items-center gap-1 flex-wrap">
          <button
            onClick={() => fetchDirectory('/')}
            className="text-xs font-medium text-amber-600 hover:text-amber-800 hover:underline"
          >
            /
          </button>
          {breadcrumbs.map((crumb, i) => (
            <span key={crumb.path} className="flex items-center gap-1">
              <ChevronRightIcon className="w-3 h-3 text-gray-400" />
              <button
                onClick={() => fetchDirectory(crumb.path)}
                className={`text-xs font-medium ${
                  i === breadcrumbs.length - 1
                    ? 'text-gray-900'
                    : 'text-amber-600 hover:text-amber-800 hover:underline'
                }`}
              >
                {crumb.label}
              </button>
            </span>
          ))}
          <label className="ml-auto flex items-center gap-1.5 text-xs text-gray-500 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={showHidden}
              onChange={(e) => setShowHidden(e.target.checked)}
              className="rounded border-gray-300 text-amber-500 focus:ring-amber-400"
            />
            Show hidden
          </label>
        </div>

        {/* Directory listing */}
        <div className="flex-1 overflow-y-auto border-t border-gray-100 min-h-0">
          {isLoadingDirs ? (
            <div className="flex items-center justify-center py-12">
              <div className="w-6 h-6 border-2 border-gray-200 border-t-amber-500 rounded-full animate-spin" />
            </div>
          ) : browseError ? (
            <div className="px-5 py-8 text-center">
              <p className="text-sm text-red-500">{browseError}</p>
            </div>
          ) : (
            <div className="py-1">
              {/* Parent directory row */}
              {parentPath && (
                <button
                  onClick={() => fetchDirectory(parentPath)}
                  className="w-full px-5 py-2.5 flex items-center gap-3 hover:bg-amber-50 text-left"
                >
                  <svg className="w-4 h-4 text-gray-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 10l7-7m0 0l7 7m-7-7v18" />
                  </svg>
                  <span className="text-sm text-gray-600">..</span>
                </button>
              )}

              {/* Directory rows */}
              {directories.length === 0 && !parentPath ? (
                <div className="px-5 py-8 text-center">
                  <p className="text-sm text-gray-400">No subdirectories</p>
                </div>
              ) : directories.length === 0 && parentPath ? (
                <div className="px-5 py-6 text-center">
                  <p className="text-sm text-gray-400">No subdirectories</p>
                </div>
              ) : (
                directories.map((dir) => (
                  <button
                    key={dir.path}
                    onClick={() => fetchDirectory(dir.path)}
                    className="w-full px-5 py-2.5 flex items-center gap-3 hover:bg-amber-50 text-left"
                  >
                    <FolderIcon className="w-4 h-4 text-amber-500 flex-shrink-0" />
                    <span className="text-sm text-gray-800 truncate">{dir.name}</span>
                  </button>
                ))
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-5 py-4 border-t border-gray-200 flex items-center gap-3">
          <div className="flex-1 min-w-0">
            <p className="text-xs font-mono text-gray-500 truncate" title={currentPath}>
              {currentPath}
            </p>
          </div>
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium border border-gray-300 bg-white hover:bg-gray-50 rounded-lg text-gray-700"
          >
            Cancel
          </button>
          <button
            onClick={handleSelect}
            disabled={!currentPath || isCreating}
            className="px-4 py-2 text-sm font-medium bg-amber-500 hover:bg-amber-600 disabled:bg-gray-300 disabled:cursor-not-allowed text-white rounded-lg"
          >
            {isCreating ? 'Creating...' : 'Select This Directory'}
          </button>
        </div>
      </div>
    </div>
  );

  return createPortal(modalContent, document.body);
}
