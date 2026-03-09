import { Link, useLocation } from 'react-router-dom';
import { MagnifyingGlassIcon, Cog6ToothIcon } from '@heroicons/react/24/outline';

export function AppNavBar() {
  const location = useLocation();

  const isActive = (path: string) => {
    if (path === '/chat') {
      return location.pathname === '/chat' || location.pathname === '/';
    }
    return location.pathname.startsWith(path);
  };

  return (
    <nav className="fixed top-0 left-0 right-0 h-14 bg-white border-b border-gray-200 z-50 shadow-sm">
      <div className="h-full max-w-[1400px] mx-auto px-6 flex items-center justify-between">
        {/* Left: Brand and Navigation */}
        <div className="flex items-center gap-6">
          {/* Brand/Logo */}
          <Link to="/chat" className="flex items-center gap-2 hover:opacity-80 transition-opacity">
            <img src="/icon_blue.png" alt="OpenDev" className="w-7 h-7 rounded-lg shadow-sm" />
            <span className="text-base font-semibold text-gray-900">OpenDev</span>
          </Link>

          {/* Navigation Links */}
          <div className="flex items-center gap-1">
            <Link
              to="/chat"
              className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${
                isActive('/chat')
                  ? 'bg-purple-100 text-purple-900'
                  : 'text-gray-700 hover:bg-gray-100 hover:text-gray-900'
              }`}
            >
              Chat
            </Link>
            <Link
              to="/codewiki"
              className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${
                isActive('/codewiki')
                  ? 'bg-purple-100 text-purple-900'
                  : 'text-gray-700 hover:bg-gray-100 hover:text-gray-900'
              }`}
            >
              CodeWiki
            </Link>
          </div>
        </div>

        {/* Right: Actions */}
        <div className="flex items-center gap-2">
          <button
            className="p-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors"
            title="Search"
          >
            <MagnifyingGlassIcon className="w-5 h-5" />
          </button>
          <button
            className="p-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors"
            title="Settings"
          >
            <Cog6ToothIcon className="w-5 h-5" />
          </button>
        </div>
      </div>
    </nav>
  );
}