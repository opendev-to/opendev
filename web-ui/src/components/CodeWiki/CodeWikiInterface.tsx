import { useState } from 'react';
import {
  MagnifyingGlassIcon,
  PlusIcon,
  Cog6ToothIcon
} from '@heroicons/react/24/outline';
import { RepositoryExplorer } from './RepositoryExplorer';
import { DocumentationViewer } from './DocumentationViewer';

export function CodeWikiInterface() {
  const [selectedRepo, setSelectedRepo] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [_isIndexing, setIsIndexing] = useState(false);

  return (
    <div className="h-full flex">
      {/* Left Sidebar - Repository Explorer */}
      <div className="w-80 bg-gray-50 border-r border-gray-200 flex flex-col">
        {/* Header */}
        <div className="p-4 border-b border-gray-200">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-gray-900">CodeWiki</h2>
            <button className="p-2 text-gray-500 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors">
              <Cog6ToothIcon className="w-4 h-4" />
            </button>
          </div>

          {/* Add Repository Button */}
          <button className="w-full px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white text-sm font-medium rounded-lg transition-colors flex items-center justify-center gap-2 mb-3">
            <PlusIcon className="w-4 h-4" />
            <span>Add Repository</span>
          </button>

          {/* Search Bar */}
          <div className="relative">
            <MagnifyingGlassIcon className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-gray-400" />
            <input
              type="text"
              placeholder="Search documentation..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-10 pr-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-transparent text-sm"
            />
          </div>
        </div>

        {/* Repository List */}
        <RepositoryExplorer
          selectedRepo={selectedRepo}
          onRepoSelect={setSelectedRepo}
          searchQuery={searchQuery}
        />
      </div>

      {/* Main Content - Documentation Viewer */}
      <div className="flex-1 flex flex-col bg-white">
        <DocumentationViewer
          selectedRepo={selectedRepo}
          searchQuery={searchQuery}
          onIndexingChange={setIsIndexing}
        />
      </div>
    </div>
  );
}