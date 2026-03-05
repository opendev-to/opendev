import { useState, useRef, useEffect } from 'react';

interface ThinkingBlockProps {
  content: string;
  level?: string;
}

const LEVEL_COLORS: Record<string, string> = {
  Low: 'bg-emerald-500/15 text-emerald-400',
  Medium: 'bg-indigo-500/15 text-indigo-400',
  High: 'bg-purple-500/15 text-purple-400',
};

export function ThinkingBlock({ content, level }: ThinkingBlockProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const [contentHeight, setContentHeight] = useState(0);

  const isCritique = content.startsWith('[Critique]');
  const accentColor = isCritique ? 'border-l-amber-500/70' : 'border-l-indigo-500/70';
  const levelBadge = level && LEVEL_COLORS[level] ? level : null;

  // Preview: first 2 lines
  const previewLines = content.split('\n').slice(0, 2).join(' ').substring(0, 160);

  useEffect(() => {
    if (contentRef.current) {
      setContentHeight(contentRef.current.scrollHeight);
    }
  }, [content]);

  return (
    <div className="animate-slide-up">
      <div className={`border-l-2 ${accentColor} rounded-r-lg overflow-hidden bg-bg-100/50`}>
        {/* Header */}
        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className="w-full px-3 py-2 flex items-center gap-2 text-left hover:bg-bg-200/50 transition-colors cursor-pointer"
        >
          {/* Brain icon */}
          <svg
            className={`w-3.5 h-3.5 flex-shrink-0 transition-colors ${isCritique ? 'text-amber-400/70' : 'text-indigo-400/70'}`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M9.75 3.104v5.714a2.25 2.25 0 0 1-.659 1.591L5 14.5M9.75 3.104c-.251.023-.501.05-.75.082m.75-.082a24.301 24.301 0 0 1 4.5 0m0 0v5.714a2.25 2.25 0 0 0 .659 1.591L19 14.5m-4.75-11.396c.251.023.501.05.75.082M12 3c2.5 0 5 .5 7 1.5M12 3c-2.5 0-5 .5-7 1.5m14 0v3m-14-3v3"
            />
          </svg>

          <span className="text-xs font-medium text-text-400 uppercase tracking-wide">
            {isCritique ? 'Critique' : 'Thinking'}
          </span>

          {/* Level badge */}
          {levelBadge && (
            <span className={`text-[10px] font-medium px-1.5 py-0.5 rounded-full ${LEVEL_COLORS[levelBadge]}`}>
              {levelBadge}
            </span>
          )}

          {/* Chevron */}
          <svg
            className={`w-3 h-3 text-text-500 transition-transform duration-200 flex-shrink-0 ml-auto ${
              isExpanded ? 'rotate-90' : ''
            }`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>

        {/* Collapsed preview */}
        {!isExpanded && (
          <div className="px-3 pb-2 relative">
            <p className="text-xs text-text-500 font-mono line-clamp-2 leading-relaxed">
              {previewLines}
            </p>
            <div className="absolute bottom-0 left-0 right-0 h-4 bg-gradient-to-t from-bg-100/50 to-transparent pointer-events-none" />
          </div>
        )}

        {/* Expanded content with smooth transition */}
        <div
          className="overflow-hidden transition-all duration-300 ease-in-out"
          style={{
            maxHeight: isExpanded ? `${Math.min(contentHeight + 24, 384)}px` : '0px',
            opacity: isExpanded ? 1 : 0,
          }}
        >
          <div ref={contentRef} className="px-3 pb-3">
            <pre className="text-xs text-text-300 whitespace-pre-wrap font-mono leading-relaxed max-h-96 overflow-y-auto">
              {content}
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
}
