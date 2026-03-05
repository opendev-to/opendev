import { ArrowLeftIcon, ArrowRightIcon } from '@heroicons/react/24/outline';

interface FooterNavProps {
  previousPage?: {
    title: string;
    onClick: () => void;
  };
  nextPage?: {
    title: string;
    onClick: () => void;
  };
  lastUpdated?: string;
}

export function FooterNav({ previousPage, nextPage, lastUpdated }: FooterNavProps) {
  return (
    <div className="border-t border-gray-200 mt-12 pt-8">
      {/* Previous/Next Navigation */}
      <div className="flex items-center justify-between mb-6">
        {previousPage ? (
          <button
            onClick={previousPage.onClick}
            className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 transition-colors group"
          >
            <ArrowLeftIcon className="w-4 h-4 group-hover:-translate-x-1 transition-transform" />
            <div className="text-left">
              <div className="text-xs text-gray-500 mb-1">Previous</div>
              <div className="font-medium">{previousPage.title}</div>
            </div>
          </button>
        ) : (
          <div />
        )}

        {nextPage ? (
          <button
            onClick={nextPage.onClick}
            className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 transition-colors group"
          >
            <div className="text-right">
              <div className="text-xs text-gray-500 mb-1">Next</div>
              <div className="font-medium">{nextPage.title}</div>
            </div>
            <ArrowRightIcon className="w-4 h-4 group-hover:translate-x-1 transition-transform" />
          </button>
        ) : (
          <div />
        )}
      </div>

      {/* Last Updated */}
      {lastUpdated && (
        <div className="text-xs text-gray-500 text-center pb-4">
          Last updated: {lastUpdated}
        </div>
      )}
    </div>
  );
}