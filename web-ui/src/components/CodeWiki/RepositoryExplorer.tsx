import { useState } from 'react';
import {
  FolderIcon,
  DocumentTextIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  StarIcon,
  ClockIcon,
  CheckCircleIcon,
  ExclamationCircleIcon,
  ArrowPathIcon
} from '@heroicons/react/24/outline';

export interface Repository {
  id: string;
  name: string;
  fullName: string;
  description: string;
  language: string;
  stars: number;
  lastIndexed: string;
  status: 'indexed' | 'indexing' | 'error';
  files: number;
  docsFound: number;
  localPath?: string;
}

interface RepositoryExplorerProps {
  selectedRepo: string | null;
  onRepoSelect: (repoId: string | null) => void;
  searchQuery: string;
}

// Mock data for development
const mockRepositories: Repository[] = [
  {
    id: '1',
    name: 'react',
    fullName: 'facebook/react',
    description: 'A declarative, efficient, and flexible JavaScript library for building user interfaces.',
    language: 'JavaScript',
    stars: 220000,
    lastIndexed: '2 hours ago',
    status: 'indexed',
    files: 3456,
    docsFound: 234
  },
  {
    id: '2',
    name: 'swe-cli',
    fullName: 'swe-cli/swe-cli',
    description: 'Software Engineering CLI with AI-powered coding assistance.',
    language: 'Python',
    stars: 1250,
    lastIndexed: '1 day ago',
    status: 'indexed',
    files: 892,
    docsFound: 67,
    localPath: '/Users/quocnghi/codes/swe-cli'
  },
  {
    id: '3',
    name: 'next.js',
    fullName: 'vercel/next.js',
    description: 'The React Framework for Production.',
    language: 'TypeScript',
    stars: 125000,
    lastIndexed: '3 days ago',
    status: 'error',
    files: 2341,
    docsFound: 0
  }
];

export function RepositoryExplorer({ selectedRepo, onRepoSelect, searchQuery }: RepositoryExplorerProps) {
  const [expandedRepos, setExpandedRepos] = useState<Set<string>>(new Set());

  const filteredRepos = mockRepositories.filter(repo =>
    repo.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    repo.fullName.toLowerCase().includes(searchQuery.toLowerCase()) ||
    repo.description.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const toggleExpanded = (repoId: string) => {
    setExpandedRepos(prev => {
      const next = new Set(prev);
      if (next.has(repoId)) {
        next.delete(repoId);
      } else {
        next.add(repoId);
      }
      return next;
    });
  };

  const getStatusIcon = (status: Repository['status']) => {
    switch (status) {
      case 'indexed':
        return <CheckCircleIcon className="w-4 h-4 text-green-500" />;
      case 'indexing':
        return <ArrowPathIcon className="w-4 h-4 text-blue-500 animate-spin" />;
      case 'error':
        return <ExclamationCircleIcon className="w-4 h-4 text-red-500" />;
    }
  };

  const getStatusText = (status: Repository['status']) => {
    switch (status) {
      case 'indexed':
        return 'Indexed';
      case 'indexing':
        return 'Indexing...';
      case 'error':
        return 'Error';
    }
  };

  const formatNumber = (num: number) => {
    if (num >= 1000) {
      return `${(num / 1000).toFixed(1)}k`;
    }
    return num.toString();
  };

  if (filteredRepos.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center p-8 text-center">
        <div className="w-16 h-16 rounded-full bg-gray-100 flex items-center justify-center mb-4">
          <DocumentTextIcon className="w-8 h-8 text-gray-400" />
        </div>
        <h3 className="text-sm font-medium text-gray-900 mb-1">No repositories found</h3>
        <p className="text-xs text-gray-500 max-w-[200px]">
          {searchQuery ? 'Try adjusting your search terms' : 'Add a repository to get started'}
        </p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-3 space-y-2">
      {/* Repository Stats Summary */}
      <div className="px-3 py-2 bg-purple-50 rounded-lg border border-purple-200">
        <div className="text-xs font-medium text-purple-900 mb-1">Repository Summary</div>
        <div className="text-xs text-purple-700">
          {filteredRepos.length} repos • {filteredRepos.reduce((sum, repo) => sum + repo.files, 0).toLocaleString()} files
        </div>
      </div>

      {/* Repository List */}
      {filteredRepos.map((repo) => {
        const isExpanded = expandedRepos.has(repo.id);
        const isSelected = selectedRepo === repo.id;

        return (
          <div
            key={repo.id}
            className={`rounded-lg border transition-all duration-200 ${
              isSelected
                ? 'border-purple-500 bg-purple-50 shadow-sm'
                : 'border-gray-200 bg-white hover:border-gray-300 hover:shadow-sm'
            }`}
          >
            {/* Repository Header */}
            <button
              onClick={() => onRepoSelect(isSelected ? null : repo.id)}
              className="w-full p-3 text-left"
            >
              <div className="flex items-start gap-3">
                {/* Expand/Collapse Chevron */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleExpanded(repo.id);
                  }}
                  className="mt-1 flex-shrink-0 text-gray-400 hover:text-gray-600"
                >
                  {isExpanded ? (
                    <ChevronDownIcon className="w-4 h-4" />
                  ) : (
                    <ChevronRightIcon className="w-4 h-4" />
                  )}
                </button>

                {/* Repository Icon */}
                <div className="mt-0.5 flex-shrink-0">
                  <FolderIcon className="w-5 h-5 text-gray-500" />
                </div>

                {/* Repository Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <h3 className="font-medium text-gray-900 text-sm truncate">{repo.name}</h3>
                    <span className={`px-2 py-0.5 text-xs rounded-full ${
                      repo.localPath
                        ? 'bg-blue-100 text-blue-700'
                        : 'bg-gray-100 text-gray-700'
                    }`}>
                      {repo.localPath ? 'Local' : 'Remote'}
                    </span>
                  </div>
                  <p className="text-xs text-gray-600 mb-2 line-clamp-2">{repo.description}</p>

                  <div className="flex items-center gap-4 text-xs text-gray-500">
                    <span className="flex items-center gap-1">
                      <StarIcon className="w-3 h-3" />
                      {formatNumber(repo.stars)}
                    </span>
                    <span className="flex items-center gap-1">
                      {getStatusIcon(repo.status)}
                      {getStatusText(repo.status)}
                    </span>
                    {repo.lastIndexed && (
                      <span className="flex items-center gap-1">
                        <ClockIcon className="w-3 h-3" />
                        {repo.lastIndexed}
                      </span>
                    )}
                  </div>
                </div>
              </div>
            </button>

            {/* Expanded Details */}
            {isExpanded && (
              <div className="px-3 pb-3 border-t border-gray-100 pt-3">
                <div className="grid grid-cols-2 gap-3 text-xs">
                  <div>
                    <span className="text-gray-500">Language:</span>
                    <span className="ml-2 font-medium text-gray-900">{repo.language}</span>
                  </div>
                  <div>
                    <span className="text-gray-500">Files:</span>
                    <span className="ml-2 font-medium text-gray-900">{repo.files.toLocaleString()}</span>
                  </div>
                  <div>
                    <span className="text-gray-500">Documents:</span>
                    <span className="ml-2 font-medium text-gray-900">{repo.docsFound}</span>
                  </div>
                  <div>
                    <span className="text-gray-500">Full Name:</span>
                    <span className="ml-2 font-medium text-gray-900 truncate">{repo.fullName}</span>
                  </div>
                </div>
                {repo.localPath && (
                  <div className="mt-2 text-xs">
                    <span className="text-gray-500">Local Path:</span>
                    <span className="ml-2 font-mono text-gray-900">{repo.localPath}</span>
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}