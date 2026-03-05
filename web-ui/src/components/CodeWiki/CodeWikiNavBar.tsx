import { Link } from 'react-router-dom';
import { ArrowLeftIcon, MagnifyingGlassIcon, Cog6ToothIcon } from '@heroicons/react/24/outline';

interface CodeWikiNavBarProps {
  repoName: string;
  onSearch?: (query: string) => void;
}

export function CodeWikiNavBar({ repoName, onSearch }: CodeWikiNavBarProps) {
  return (
    <nav className="fixed top-0 left-0 right-0 h-16 bg-white border-b border-gray-200 z-50">
      <div className="h-full max-w-7xl mx-auto px-6 flex items-center justify-between">
        {/* Left: Breadcrumb */}
        <div className="flex items-center gap-2 text-sm">
          <Link
            to="/codewiki"
            className="text-gray-600 hover:text-gray-900 transition-colors font-medium"
          >
            CodeWiki
          </Link>
          <span className="text-gray-400">/</span>
          <span className="text-gray-900 font-semibold">{repoName}</span>
        </div>

        {/* Center: Search Bar (optional) */}
        {onSearch && (
          <div className="flex-1 max-w-md mx-8">
            <div className="relative">
              <MagnifyingGlassIcon className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-gray-400" />
              <input
                type="text"
                placeholder="Search documentation..."
                onChange={(e) => onSearch(e.target.value)}
                className="w-full pl-10 pr-4 py-2 text-sm border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-transparent"
              />
            </div>
          </div>
        )}

        {/* Right: Actions */}
        <div className="flex items-center gap-3">
          <button className="p-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors">
            <Cog6ToothIcon className="w-5 h-5" />
          </button>
          <Link
            to="/codewiki"
            className="flex items-center gap-2 px-4 py-2 text-gray-700 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors font-medium"
          >
            <ArrowLeftIcon className="w-4 h-4" />
            Back
          </Link>
        </div>
      </div>
    </nav>
  );
}