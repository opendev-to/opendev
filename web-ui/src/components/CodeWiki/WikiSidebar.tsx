import { useState } from 'react';
import {
  DocumentTextIcon,
  FolderIcon,
  ChevronRightIcon,
  ChevronDownIcon,
  TagIcon,
  CodeBracketIcon
} from '@heroicons/react/24/outline';

interface WikiPage {
  id: string;
  title: string;
  type: 'architecture' | 'api' | 'guide' | 'reference' | 'overview';
  children?: WikiPage[];
  parent?: string;
}

interface WikiSidebarProps {
  wikiPages: WikiPage[];
  selectedPageId: string | null;
  onPageSelect: (pageId: string) => void;
}

export function WikiSidebar({ wikiPages, selectedPageId, onPageSelect }: WikiSidebarProps) {
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set(['wiki-pages']));

  const toggleSection = (section: string) => {
    setExpandedSections(prev => {
      const next = new Set(prev);
      if (next.has(section)) {
        next.delete(section);
      } else {
        next.add(section);
      }
      return next;
    });
  };

  const getPageIcon = (type: WikiPage['type']) => {
    switch (type) {
      case 'overview':
        return <DocumentTextIcon className="w-4 h-4" />;
      case 'architecture':
        return <FolderIcon className="w-4 h-4" />;
      case 'api':
        return <CodeBracketIcon className="w-4 h-4" />;
      default:
        return <DocumentTextIcon className="w-4 h-4" />;
    }
  };

  const renderWikiPage = (page: WikiPage, depth: number = 0) => {
    const isSelected = selectedPageId === page.id;
    const hasChildren = page.children && page.children.length > 0;
    const isExpanded = expandedSections.has(`page-${page.id}`);

    return (
      <div key={page.id}>
        <button
          onClick={() => {
            onPageSelect(page.id);
            if (hasChildren) {
              toggleSection(`page-${page.id}`);
            }
          }}
          className={`w-full flex items-center gap-2 px-3 py-2 text-sm rounded-lg transition-colors ${
            isSelected
              ? 'bg-purple-100 text-purple-900 font-medium'
              : 'text-gray-700 hover:bg-gray-100'
          }`}
          style={{ paddingLeft: `${12 + depth * 16}px` }}
        >
          {hasChildren && (
            <span className="flex-shrink-0">
              {isExpanded ? (
                <ChevronDownIcon className="w-3 h-3" />
              ) : (
                <ChevronRightIcon className="w-3 h-3" />
              )}
            </span>
          )}
          <span className="flex-shrink-0 text-gray-500">
            {getPageIcon(page.type)}
          </span>
          <span className="truncate">{page.title}</span>
        </button>

        {/* Render children if expanded */}
        {hasChildren && isExpanded && (
          <div className="mt-1">
            {page.children!.map(child => renderWikiPage(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  return (
    <aside className="w-64 bg-gray-50 border-r border-gray-200 overflow-y-auto sticky top-0 h-[calc(100vh-6.5rem)]">
      <div className="p-4">
        {/* Wiki Pages Section */}
        <div className="mb-6">
          <button
            onClick={() => toggleSection('wiki-pages')}
            className="w-full flex items-center justify-between px-2 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider hover:text-gray-700 transition-colors"
          >
            <span>Wiki Pages</span>
            {expandedSections.has('wiki-pages') ? (
              <ChevronDownIcon className="w-4 h-4" />
            ) : (
              <ChevronRightIcon className="w-4 h-4" />
            )}
          </button>

          {expandedSections.has('wiki-pages') && (
            <div className="mt-2 space-y-1">
              {wikiPages.filter(p => !p.parent).map(page => renderWikiPage(page))}
            </div>
          )}
        </div>

        {/* Tags Section */}
        <div className="mb-6">
          <button
            onClick={() => toggleSection('tags')}
            className="w-full flex items-center justify-between px-2 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider hover:text-gray-700 transition-colors"
          >
            <span>Tags</span>
            {expandedSections.has('tags') ? (
              <ChevronDownIcon className="w-4 h-4" />
            ) : (
              <ChevronRightIcon className="w-4 h-4" />
            )}
          </button>

          {expandedSections.has('tags') && (
            <div className="mt-2 flex flex-wrap gap-2">
              {['architecture', 'api', 'guide', 'overview'].map(tag => (
                <button
                  key={tag}
                  className="flex items-center gap-1 px-2 py-1 text-xs bg-gray-100 hover:bg-gray-200 text-gray-700 rounded-full transition-colors"
                >
                  <TagIcon className="w-3 h-3" />
                  {tag}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Related Files Section */}
        <div>
          <button
            onClick={() => toggleSection('files')}
            className="w-full flex items-center justify-between px-2 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider hover:text-gray-700 transition-colors"
          >
            <span>Related Files</span>
            {expandedSections.has('files') ? (
              <ChevronDownIcon className="w-4 h-4" />
            ) : (
              <ChevronRightIcon className="w-4 h-4" />
            )}
          </button>

          {expandedSections.has('files') && (
            <div className="mt-2 space-y-1">
              {['core/agent.py', 'web/server.py', 'tools/registry.py'].map(file => (
                <button
                  key={file}
                  className="w-full flex items-center gap-2 px-3 py-2 text-sm text-gray-700 hover:bg-gray-100 rounded-lg transition-colors text-left"
                >
                  <CodeBracketIcon className="w-4 h-4 text-gray-500 flex-shrink-0" />
                  <span className="truncate text-xs">{file}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </aside>
  );
}