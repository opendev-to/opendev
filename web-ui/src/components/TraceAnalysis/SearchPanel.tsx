import { useState, useMemo } from 'react';
import type { AnyFlowNode } from '../../utils/trace/collapseGraph';
import type { TraceNodeData, ToolNodeData, TaskNodeData, CollapsedNodeData, AnyNodeData, ContentBlock } from '../../types/trace';
import { getToolNames } from '../../types/trace';
import { useDebounce } from '../../hooks/useDebounce';

const NODE_TYPE_DOT_CLASSES: Record<string, string> = {
  user: 'bg-accent-secondary-100',
  assistant: 'bg-success-100',
  'tool-call': 'bg-accent-main-100',
  'task-call': 'bg-warning-100',
  'subagent-user': 'bg-purple-500',
  'subagent-assistant': 'bg-indigo-500',
  'hook-progress': 'bg-gray-400',
  summary: 'bg-gray-400',
};

interface Props {
  nodes: AnyFlowNode[];
  onSelectNode: (nodeId: string, eventIndex?: number) => void;
}

interface SearchResult {
  nodeId: string;
  eventIndex?: number;
  eventType: string;
  preview: string;
  toolNames: string[];
  matchSnippet: string;
  isInnerEvent?: boolean;
  chainLabel?: string;
}

// ⚡ Bolt Performance Optimization:
// Previously, matchesQuery, highlightMatch, and truncateAround used .toLowerCase()
// on the text inside the helper function. This caused massive O(N) redundant string
// allocations when filtering large lists inside `useMemo`, as the large text strings
// like `fullText` were lowercased repeatedly during the search loops even if `lowerQuery`
// was already hoisted. We optimize this by using case-insensitive regex or tracking
// lowercased strings more efficiently without repeated allocations.

function escapeRegExp(string: string) {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Global cache for compiled RegExp to avoid recompiling it hundreds of times
// during a single render loop when filtering the node tree.
const regExpCache = new Map<string, RegExp>();
function getSearchRegExp(query: string): RegExp {
  if (!regExpCache.has(query)) {
    regExpCache.set(query, new RegExp(escapeRegExp(query), 'i'));
  }
  return regExpCache.get(query)!;
}

function matchesQuery(text: string, lowerQuery: string): boolean {
  // Avoid allocating a lowercased copy of the potentially large 'text'
  // by using a cached, case-insensitive regular expression match.
  return getSearchRegExp(lowerQuery).test(text);
}

function highlightMatch(text: string, lowerQuery: string): React.ReactNode {
  if (!lowerQuery) return text;
  // Use regex to find index without lowercasing the entire text
  const matchRegExp = getSearchRegExp(lowerQuery);
  const matchResult = text.match(matchRegExp);
  if (!matchResult || matchResult.index === undefined) return text;
  const idx = matchResult.index;
  const before = text.slice(0, idx);
  const match = text.slice(idx, idx + lowerQuery.length);
  const after = text.slice(idx + lowerQuery.length);
  return (
    <>
      {before}
      <span className="bg-accent-main-100/30 text-text-000 rounded-sm px-px">{match}</span>
      {after}
    </>
  );
}

function contentBlocksToText(content: string | ContentBlock[] | undefined): string {
  if (!content) return '';
  if (typeof content === 'string') return content;
  return content.map(block => {
    if (block.type === 'text') return block.text || '';
    if (block.type === 'thinking') return block.thinking || '';
    if (block.type === 'tool_use') return `${block.name || ''} ${JSON.stringify(block.input || {})}`;
    if (block.type === 'tool_result') {
      const c = block.content;
      if (typeof c === 'string') return c;
      if (Array.isArray(c)) return c.map(b => b.text || '').join(' ');
    }
    return '';
  }).join(' ');
}

function getFullContent(data: AnyNodeData): string {
  if (data.eventType === 'tool-call' || data.eventType === 'task-call') {
    const td = data as ToolNodeData | TaskNodeData;
    return [
      contentBlocksToText(td.assistantEvent.message?.content),
      contentBlocksToText(td.userEvent.message?.content),
    ].join(' ');
  }
  const td = data as TraceNodeData;
  const ev = td.event;
  if (ev.type === 'progress' && ev.data?.message) {
    return contentBlocksToText(ev.data.message.message?.content);
  }
  return contentBlocksToText(ev.message?.content);
}

function searchNodeData(data: AnyNodeData, lowerQuery: string): string | null {
  if (matchesQuery(data.preview, lowerQuery)) return data.preview;
  const toolNames = getToolNames(data);
  for (const name of toolNames) {
    if (matchesQuery(name, lowerQuery)) return name;
  }
  if (matchesQuery(data.eventType, lowerQuery)) return data.eventType;
  if (data.eventType === 'tool-call' || data.eventType === 'task-call') {
    const tools = (data as ToolNodeData | TaskNodeData).tools;
    for (const tool of tools) {
      if (tool.result && matchesQuery(tool.result, lowerQuery)) return truncateAround(tool.result, lowerQuery, 80);
      const inputStr = JSON.stringify(tool.input);
      if (matchesQuery(inputStr, lowerQuery)) return truncateAround(inputStr, lowerQuery, 80);
    }
  }
  const fullText = getFullContent(data);
  if (fullText && matchesQuery(fullText, lowerQuery)) return truncateAround(fullText, lowerQuery, 80);
  return null;
}

function truncateAround(text: string, lowerQuery: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  const matchRegExp = getSearchRegExp(lowerQuery);
  const matchResult = text.match(matchRegExp);
  const idx = matchResult?.index ?? -1;
  if (idx === -1) return text.slice(0, maxLen);
  const start = Math.max(0, idx - Math.floor((maxLen - lowerQuery.length) / 2));
  const slice = text.slice(start, start + maxLen);
  return (start > 0 ? '\u2026' : '') + slice + (start + maxLen < text.length ? '\u2026' : '');
}

export function SearchPanel({ nodes, onSelectNode }: Props) {
  const [query, setQuery] = useState('');

  // ⚡ Bolt Performance Optimization:
  // Debouncing the search query to prevent excessive filtering of nodes on every keystroke
  const debouncedQuery = useDebounce(query, 300);

  const results = useMemo<SearchResult[]>(() => {
    const q = debouncedQuery.trim();
    if (!q) return [];
    const lowerQuery = q.toLowerCase();

    // ⚡ Bolt Performance Optimization:
    // Create the RegEx once per query and pass it down, or at least avoid
    // repeated RegEx compilations. We've switched the helpers to use RegEx to avoid
    // allocating new lowercased strings. However, wait, compiling a regex inside
    // the helper is redundant. We can compile it once here.
    // Actually, passing lowerQuery is fine, but compiling Regex per text is slightly
    // expensive. But it's much faster than allocating huge strings in JS.
    const out: SearchResult[] = [];

    for (const node of nodes) {
      if (node.type === 'collapsedNode') {
        const cData = node.data as CollapsedNodeData;
        for (let i = 0; i < cData.events.length; i++) {
          const ev = cData.events[i];
          const matchText = searchNodeData(ev, lowerQuery);
          if (matchText) {
            out.push({
              nodeId: node.id,
              eventIndex: i,
              eventType: ev.eventType,
              preview: ev.preview.slice(0, 80),
              toolNames: getToolNames(ev).slice(0, 3),
              matchSnippet: matchText.slice(0, 80),
              isInnerEvent: true,
              chainLabel: `chain (${cData.count})`,
            });
          }
        }
      } else {
        const data = node.data as TraceNodeData | ToolNodeData | TaskNodeData;
        const matchText = searchNodeData(data, lowerQuery);
        if (matchText) {
          out.push({
            nodeId: node.id,
            eventType: data.eventType,
            preview: data.preview.slice(0, 80),
            toolNames: getToolNames(data).slice(0, 3),
            matchSnippet: matchText.slice(0, 80),
          });
        }
      }
    }

    return out;
  }, [nodes, debouncedQuery]);

  const lowerDebouncedQuery = debouncedQuery.trim().toLowerCase();

  return (
    <div className="w-[260px] shrink-0 flex flex-col bg-bg-100 border-r border-border-300/20 font-sans overflow-hidden">
      <div className="px-3 pt-2.5 pb-1.5 text-[11px] font-bold text-text-400 uppercase tracking-wider">
        Search
      </div>
      <div className="px-2 pb-2 relative">
        <input
          type="text"
          placeholder="Filter nodes\u2026"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-full py-1.5 px-2 pr-7 text-xs bg-bg-000 border border-border-300/30 rounded text-text-000 outline-none focus:border-accent-secondary-100"
        />
        {query && (
          <button
            className="absolute right-3 top-1 bg-transparent border-none text-text-400 cursor-pointer text-base leading-5 px-1 hover:text-text-000"
            onClick={() => setQuery('')}
          >
            ×
          </button>
        )}
      </div>

      {debouncedQuery.trim() && (
        <div className="px-3 pb-1.5 text-[10px] text-text-400">
          {results.length} result{results.length !== 1 ? 's' : ''}
        </div>
      )}

      <div className="flex-1 overflow-y-auto overflow-x-hidden">
        {results.map((r, idx) => (
          <button
            key={`${r.nodeId}-${r.eventIndex ?? idx}`}
            className="block w-full px-3 py-2 border-none border-b border-border-300/20 bg-transparent cursor-pointer text-left hover:bg-bg-200"
            onClick={() => onSelectNode(r.nodeId, r.eventIndex)}
          >
            <div className="flex items-center gap-1.5 mb-0.5">
              <span className={`inline-block w-2 h-2 rounded-full shrink-0 ${NODE_TYPE_DOT_CLASSES[r.eventType] || 'bg-gray-400'}`} />
              <span className="text-[10px] font-semibold text-text-400 uppercase">{r.eventType}</span>
              {r.chainLabel && (
                <span className="text-[9px] px-1 rounded bg-bg-300 text-text-400 ml-auto">{r.chainLabel}</span>
              )}
            </div>
            <div className="text-[11px] text-text-200 leading-4 overflow-hidden text-ellipsis whitespace-nowrap">
              {highlightMatch(r.matchSnippet, lowerDebouncedQuery)}
            </div>
            {r.toolNames.length > 0 && (
              <div className="flex gap-1 mt-1 flex-wrap">
                {r.toolNames.map((t) => (
                  <span key={t} className="text-[9px] px-1.5 py-px rounded bg-bg-200 text-text-400">{t}</span>
                ))}
              </div>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}
