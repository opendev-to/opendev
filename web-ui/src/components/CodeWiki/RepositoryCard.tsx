import { Link } from 'react-router-dom';
import {
  FolderIcon,
  StarIcon,
  DocumentTextIcon,
  CheckCircleIcon,
  ArrowPathIcon,
  ExclamationCircleIcon,
  ClockIcon
} from '@heroicons/react/24/outline';
import { Repository } from './RepositoryExplorer';

interface RepositoryCardProps {
  repository: Repository;
}

// Language color mapping for visual indicators
const languageColors: Record<string, string> = {
  'JavaScript': 'bg-yellow-400',
  'TypeScript': 'bg-blue-400',
  'Python': 'bg-green-400',
  'Java': 'bg-red-400',
  'C++': 'bg-purple-400',
  'Go': 'bg-cyan-400',
  'Rust': 'bg-orange-400',
  'Ruby': 'bg-red-500',
  'PHP': 'bg-indigo-400',
  'Swift': 'bg-orange-500',
  'default': 'bg-gray-400'
};

export function RepositoryCard({ repository }: RepositoryCardProps) {
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

  const languageColor = languageColors[repository.language] || languageColors.default;

  return (
    <Link
      to={`/codewiki/${repository.name}`}
      className="group block bg-white rounded-xl border border-gray-200 hover:border-purple-300 hover:shadow-lg transition-all duration-300 overflow-hidden"
    >
      {/* Card Header with Gradient Background */}
      <div className="h-24 bg-gradient-to-br from-purple-500 via-purple-600 to-blue-600 p-4 relative overflow-hidden">
        {/* Background Pattern */}
        <div className="absolute inset-0 opacity-10">
          <div className="absolute -top-4 -right-4 w-20 h-20 bg-white rounded-full" />
          <div className="absolute -bottom-8 -left-8 w-16 h-16 bg-white rounded-full" />
        </div>

        {/* Repository Icon and Status */}
        <div className="relative z-10 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-white/20 backdrop-blur rounded-lg flex items-center justify-center">
              <FolderIcon className="w-6 h-6 text-white" />
            </div>
            <div>
              <h3 className="text-white font-semibold text-lg">{repository.name}</h3>
              <p className="text-white/80 text-sm">{repository.fullName}</p>
            </div>
          </div>

          <div className="flex items-center gap-1.5 px-2 py-1 bg-white/20 backdrop-blur rounded-full">
            {getStatusIcon(repository.status)}
            <span className="text-white text-xs font-medium">{getStatusText(repository.status)}</span>
          </div>
        </div>
      </div>

      {/* Card Content */}
      <div className="p-5">
        {/* Description */}
        <p className="text-gray-600 text-sm mb-4 line-clamp-2">{repository.description}</p>

        {/* Stats Grid */}
        <div className="grid grid-cols-3 gap-4 mb-4">
          <div className="text-center">
            <div className="flex items-center justify-center gap-1 text-gray-500 mb-1">
              <StarIcon className="w-4 h-4" />
            </div>
            <div className="text-sm font-semibold text-gray-900">{formatNumber(repository.stars)}</div>
            <div className="text-xs text-gray-500">Stars</div>
          </div>

          <div className="text-center">
            <div className="flex items-center justify-center gap-1 text-gray-500 mb-1">
              <DocumentTextIcon className="w-4 h-4" />
            </div>
            <div className="text-sm font-semibold text-gray-900">{repository.files.toLocaleString()}</div>
            <div className="text-xs text-gray-500">Files</div>
          </div>

          <div className="text-center">
            <div className="flex items-center justify-center gap-1 text-gray-500 mb-1">
              <DocumentTextIcon className="w-4 h-4" />
            </div>
            <div className="text-sm font-semibold text-gray-900">{repository.docsFound}</div>
            <div className="text-xs text-gray-500">Docs</div>
          </div>
        </div>

        {/* Footer with Language and Time */}
        <div className="flex items-center justify-between pt-4 border-t border-gray-100">
          <div className="flex items-center gap-2">
            <div className={`w-3 h-3 rounded-full ${languageColor}`} />
            <span className="text-sm text-gray-600 font-medium">{repository.language}</span>
            {repository.localPath && (
              <span className="px-2 py-0.5 bg-blue-100 text-blue-700 text-xs rounded-full font-medium">
                Local
              </span>
            )}
          </div>

          {repository.lastIndexed && (
            <div className="flex items-center gap-1 text-gray-500">
              <ClockIcon className="w-3 h-3" />
              <span className="text-xs">{repository.lastIndexed}</span>
            </div>
          )}
        </div>

        {/* Hover Indicator */}
        <div className="mt-3 text-center text-xs text-gray-500 opacity-0 group-hover:opacity-100 transition-opacity">
          Click to explore documentation →
        </div>
      </div>
    </Link>
  );
}