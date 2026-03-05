import { FolderIcon, CheckCircleIcon, ExclamationCircleIcon, ArrowPathIcon } from '@heroicons/react/24/outline';
import { Repository } from './RepositoryExplorer';

interface RepositoryHeroProps {
  repository: Repository;
}

// Language color mapping
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

export function RepositoryHero({ repository }: RepositoryHeroProps) {
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

  const languageColor = languageColors[repository.language] || languageColors.default;

  return (
    <div className="bg-gradient-to-br from-purple-50 via-white to-blue-50 border-b border-gray-200">
      <div className="max-w-7xl mx-auto px-6 py-8">
        <div className="flex items-start gap-4">
          {/* Repository Icon */}
          <div className="w-14 h-14 bg-gradient-to-br from-purple-500 to-blue-600 rounded-xl flex items-center justify-center shadow-md flex-shrink-0">
            <FolderIcon className="w-7 h-7 text-white" />
          </div>

          {/* Repository Info */}
          <div className="flex-1">
            <h1 className="text-3xl font-bold text-gray-900 mb-2">{repository.name}</h1>
            <p className="text-gray-600 text-lg mb-3">{repository.fullName}</p>
            <p className="text-gray-700 text-base leading-relaxed mb-4 max-w-3xl">
              {repository.description}
            </p>

            {/* Metadata Badges */}
            <div className="flex items-center gap-3 flex-wrap">
              {/* Status Badge */}
              <div className="flex items-center gap-2 px-3 py-1.5 bg-white rounded-full shadow-sm border border-gray-200">
                {getStatusIcon(repository.status)}
                <span className="text-sm font-medium text-gray-700 capitalize">{repository.status}</span>
              </div>

              {/* Language Badge */}
              <div className="flex items-center gap-2 px-3 py-1.5 bg-white rounded-full shadow-sm border border-gray-200">
                <div className={`w-3 h-3 rounded-full ${languageColor}`} />
                <span className="text-sm font-medium text-gray-700">{repository.language}</span>
              </div>

              {/* Local Path Badge (if applicable) */}
              {repository.localPath && (
                <div className="px-3 py-1.5 bg-blue-100 text-blue-700 text-sm font-medium rounded-full">
                  Local Repository
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}