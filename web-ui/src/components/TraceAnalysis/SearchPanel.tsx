import { useState, useMemo, useDeferredValue } from 'react';
import type { AnyFlowNode } from '../../utils/trace/collapseGraph';
import type { TraceNodeData, ToolNodeData, TaskNodeData, CollapsedNodeData, AnyNodeData, ContentBlock } from '../../types/trace';
import { getToolNames } from '../../types/trace';

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

function escapeRegExp(string: string) {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); // $& means the whole matched string
}

function matchesQuery(text: string, queryRegex: RegExp): boolean {
  return queryRegex.test(text);
}

function highlightMatch(text: string, queryRegex: RegExp | null): React.ReactNode {
  if (!queryRegex) return text;
  queryRegex.lastIndex = 0;
  const match = queryRegex.exec(text);
  if (!match) return text;
  const idx = match.index;
  const before = text.slice(0, idx);
  const matchedText = match[0];
  const after = text.slice(idx + matchedText.length);
  return (
    <>
      {before}
      <span className="bg-accent-main-100/30 text-text-000 rounded-sm px-px">{matchedText}</span>
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

function searchNodeData(data: AnyNodeData, queryRegex: RegExp): string | null {
  if (matchesQuery(data.preview, queryRegex)) return data.preview;
  const toolNames = getToolNames(data);
  for (const name of toolNames) {
    if (matchesQuery(name, queryRegex)) return name;
  }
  if (matchesQuery(data.eventType, queryRegex)) return data.eventType;
  if (data.eventType === 'tool-call' || data.eventType === 'task-call') {
    const tools = (data as ToolNodeData | TaskNodeData).tools;
    for (const tool of tools) {
      if (tool.result && matchesQuery(tool.result, queryRegex)) return truncateAround(tool.result, queryRegex, 80);
      const inputStr = JSON.stringify(tool.input);
      if (matchesQuery(inputStr, queryRegex)) return truncateAround(inputStr, queryRegex, 80);
    }
  }
  const fullText = getFullContent(data);
  if (fullText && matchesQuery(fullText, queryRegex)) return truncateAround(fullText, queryRegex, 80);
  return null;
}

function truncateAround(text: string, queryRegex: RegExp, maxLen: number): string {
  if (text.length <= maxLen) return text;
  queryRegex.lastIndex = 0;
  const match = queryRegex.exec(text);
  if (!match) return text.slice(0, maxLen);
  const idx = match.index;
  const start = Math.max(0, idx - Math.floor((maxLen - match[0].length) / 2));
  const slice = text.slice(start, start + maxLen);
  return (start > 0 ? '\u2026' : '') + slice + (start + maxLen < text.length ? '\u2026' : '');
}

export function SearchPanel({ nodes, onSelectNode }: Props) {
  const [query, setQuery] = useState('');

  // ⚡ Bolt Performance Optimization:
  // Use useDeferredValue instead of useDebounce for local array filtering.
  // This prevents artificial UI lag (waiting 300ms) and allows React to prioritize
  // typing updates while rendering the filtered list in the background.
  const deferredQuery = useDeferredValue(query);

  const results = useMemo<SearchResult[]>(() => {
    const q = deferredQuery.trim();
    if (!q) return [];
    const queryRegex = new RegExp(escapeRegExp(q), 'i');
    const out: SearchResult[] = [];

    for (const node of nodes) {
      if (node.type === 'collapsedNode') {
        const cData = node.data as CollapsedNodeData;
        for (let i = 0; i < cData.events.length; i++) {
          const ev = cData.events[i];
          const matchText = searchNodeData(ev, queryRegex);
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
        const matchText = searchNodeData(data, queryRegex);
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
  }, [nodes, deferredQuery]);

  const queryRegex = useMemo(() => deferredQuery.trim() ? new RegExp(escapeRegExp(deferredQuery.trim()), 'i') : null, [deferredQuery]);

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

      {deferredQuery.trim() && (
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
              {highlightMatch(r.matchSnippet, queryRegex)}
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
