import { MagnifyingGlassIcon, PlusIcon, Cog6ToothIcon } from '@heroicons/react/24/outline';
import { RepositoryCard } from './RepositoryCard';
import { Repository } from './RepositoryExplorer';

interface RepositoryGridProps {
  repositories: Repository[];
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onAddRepository: () => void;
}

export function RepositoryGrid({ repositories, searchQuery, onSearchChange, onAddRepository }: RepositoryGridProps) {
  const filteredRepositories = repositories.filter(repo =>
    repo.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    repo.fullName.toLowerCase().includes(searchQuery.toLowerCase()) ||
    repo.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
    repo.language.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const totalFiles = filteredRepositories.reduce((sum, repo) => sum + repo.files, 0);
  const totalDocs = filteredRepositories.reduce((sum, repo) => sum + repo.docsFound, 0);

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Header Section */}
      <div className="bg-white border-b border-gray-200">
        <div className="max-w-7xl mx-auto px-6 py-6">
          {/* Page Title and Actions */}
          <div className="flex items-center justify-between mb-6">
            <div>
              <h1 className="text-3xl font-bold text-gray-900">CodeWiki</h1>
              <p className="text-gray-600 mt-1">Explore intelligent documentation for your repositories</p>
            </div>

            <div className="flex items-center gap-3">
              <button className="p-2 text-gray-500 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors">
                <Cog6ToothIcon className="w-5 h-5" />
              </button>
              <button
                onClick={onAddRepository}
                className="px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-lg transition-colors flex items-center gap-2"
              >
                <PlusIcon className="w-4 h-4" />
                Add Repository
              </button>
            </div>
          </div>

          {/* Search Bar */}
          <div className="relative max-w-2xl">
            <MagnifyingGlassIcon className="absolute left-3 top-1/2 transform -translate-y-1/2 w-5 h-5 text-gray-400" />
            <input
              type="text"
              placeholder="Search repositories by name, language, or description..."
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              className="w-full pl-10 pr-4 py-3 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-transparent text-sm placeholder-gray-500"
            />
          </div>
        </div>
      </div>

      {/* Stats Bar */}
      <div className="bg-white border-b border-gray-200">
        <div className="max-w-7xl mx-auto px-6 py-4">
          <div className="flex items-center gap-8 text-sm">
            <div className="flex items-center gap-2">
              <span className="text-gray-500">Repositories:</span>
              <span className="font-semibold text-gray-900">{filteredRepositories.length}</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-gray-500">Total Files:</span>
              <span className="font-semibold text-gray-900">{totalFiles.toLocaleString()}</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-gray-500">Documents:</span>
              <span className="font-semibold text-gray-900">{totalDocs.toLocaleString()}</span>
            </div>
          </div>
        </div>
      </div>

      {/* Repository Cards Grid */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        {filteredRepositories.length === 0 ? (
          <div className="text-center py-16">
            <div className="w-20 h-20 mx-auto mb-6 bg-gray-100 rounded-full flex items-center justify-center">
              <MagnifyingGlassIcon className="w-10 h-10 text-gray-400" />
            </div>
            <h3 className="text-xl font-semibold text-gray-900 mb-2">
              {searchQuery ? 'No repositories found' : 'No repositories yet'}
            </h3>
            <p className="text-gray-600 mb-6 max-w-md mx-auto">
              {searchQuery
                ? 'Try adjusting your search terms or add new repositories to explore.'
                : 'Add your first repository to start exploring intelligent documentation.'
              }
            </p>
            {!searchQuery && (
              <button
                onClick={onAddRepository}
                className="px-6 py-3 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-lg transition-colors inline-flex items-center gap-2"
              >
                <PlusIcon className="w-5 h-5" />
                Add Your First Repository
              </button>
            )}
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {filteredRepositories.map((repository) => (
              <RepositoryCard key={repository.id} repository={repository} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}