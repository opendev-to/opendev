import ReactMarkdown from 'react-markdown';
import { CodeBracketIcon, DocumentTextIcon } from '@heroicons/react/24/outline';

interface WikiPage {
  id: string;
  title: string;
  type: 'architecture' | 'api' | 'guide' | 'reference' | 'overview';
  content: string;
  description?: string;
  lastModified: string;
  relatedFiles?: string[];
  relatedPages?: string[];
  tags?: string[];
}

interface DocumentationContentProps {
  wikiPage: WikiPage;
}

export function DocumentationContent({ wikiPage }: DocumentationContentProps) {
  return (
    <div className="max-w-4xl mx-auto px-8 py-8">
      {/* Page Header */}
      <div className="mb-8">
        <h1 className="text-4xl font-bold text-gray-900 mb-4 leading-tight">
          {wikiPage.title}
        </h1>

        {wikiPage.description && (
          <p className="text-xl text-gray-600 leading-relaxed mb-6">
            {wikiPage.description}
          </p>
        )}

        {/* Metadata */}
        <div className="flex items-center gap-4 text-sm text-gray-500">
          <span className="px-3 py-1 bg-purple-100 text-purple-700 rounded-full font-medium capitalize">
            {wikiPage.type}
          </span>
          <span>Last updated: {wikiPage.lastModified}</span>
        </div>

        {/* Tags */}
        {wikiPage.tags && wikiPage.tags.length > 0 && (
          <div className="flex gap-2 flex-wrap mt-4">
            {wikiPage.tags.map(tag => (
              <span
                key={tag}
                className="px-3 py-1 bg-gray-100 text-gray-700 rounded-full text-sm hover:bg-gray-200 transition-colors cursor-pointer"
              >
                #{tag}
              </span>
            ))}
          </div>
        )}
      </div>

      {/* Divider */}
      <div className="border-t border-gray-200 mb-8" />

      {/* Main Content with Enhanced Typography */}
      <article className="prose prose-lg max-w-none">
        <ReactMarkdown
          className="text-gray-800 leading-relaxed"
          components={{
            h1: ({ children, ...props }) => (
              <h1 className="text-3xl font-bold text-gray-900 mt-10 mb-6 leading-tight" {...props}>
                {children}
              </h1>
            ),
            h2: ({ children, ...props }) => (
              <h2 className="text-2xl font-bold text-gray-900 mt-8 mb-4 leading-tight" {...props}>
                {children}
              </h2>
            ),
            h3: ({ children, ...props }) => (
              <h3 className="text-xl font-semibold text-gray-900 mt-6 mb-3 leading-tight" {...props}>
                {children}
              </h3>
            ),
            p: ({ children, ...props }) => (
              <p className="text-base text-gray-700 mb-4 leading-relaxed" {...props}>
                {children}
              </p>
            ),
            ul: ({ children, ...props }) => (
              <ul className="list-disc pl-6 mb-4 space-y-2" {...props}>
                {children}
              </ul>
            ),
            ol: ({ children, ...props }) => (
              <ol className="list-decimal pl-6 mb-4 space-y-2" {...props}>
                {children}
              </ol>
            ),
            li: ({ children, ...props }) => (
              <li className="text-gray-700 leading-relaxed" {...props}>
                {children}
              </li>
            ),
            code: ({ children, className, ...props }) => {
              const isInline = !className;
              return isInline ? (
                <code
                  className="bg-purple-50 text-purple-900 px-1.5 py-0.5 rounded text-sm font-mono"
                  {...props}
                >
                  {children}
                </code>
              ) : (
                <code
                  className="block bg-gray-900 text-gray-100 p-4 rounded-lg overflow-x-auto text-sm font-mono leading-relaxed"
                  {...props}
                >
                  {children}
                </code>
              );
            },
            blockquote: ({ children, ...props }) => (
              <blockquote
                className="border-l-4 border-purple-500 pl-4 py-2 my-4 bg-purple-50 text-gray-700 italic"
                {...props}
              >
                {children}
              </blockquote>
            ),
            a: ({ children, ...props }) => (
              <a
                className="text-purple-600 hover:text-purple-800 underline"
                {...props}
              >
                {children}
              </a>
            ),
          }}
        >
          {wikiPage.content}
        </ReactMarkdown>
      </article>

      {/* Related Content Sections */}
      <div className="mt-12 space-y-6">
        {/* Related Files */}
        {wikiPage.relatedFiles && wikiPage.relatedFiles.length > 0 && (
          <div className="bg-blue-50 rounded-xl p-6 border border-blue-200">
            <h3 className="flex items-center gap-2 text-lg font-semibold text-blue-900 mb-4">
              <CodeBracketIcon className="w-5 h-5" />
              Related Files
            </h3>
            <div className="space-y-2">
              {wikiPage.relatedFiles.map((file, index) => (
                <div
                  key={index}
                  className="flex items-center gap-3 text-sm bg-white px-4 py-3 rounded-lg hover:shadow-sm transition-shadow cursor-pointer"
                >
                  <CodeBracketIcon className="w-4 h-4 text-blue-600 flex-shrink-0" />
                  <code className="text-blue-800 font-mono">{file}</code>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Related Pages */}
        {wikiPage.relatedPages && wikiPage.relatedPages.length > 0 && (
          <div className="bg-purple-50 rounded-xl p-6 border border-purple-200">
            <h3 className="flex items-center gap-2 text-lg font-semibold text-purple-900 mb-4">
              <DocumentTextIcon className="w-5 h-5" />
              Related Documentation
            </h3>
            <div className="flex flex-wrap gap-3">
              {wikiPage.relatedPages.map((page, index) => (
                <button
                  key={index}
                  className="px-4 py-2 bg-white text-purple-700 rounded-lg hover:bg-purple-100 hover:shadow-sm transition-all font-medium text-sm"
                >
                  {page}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}