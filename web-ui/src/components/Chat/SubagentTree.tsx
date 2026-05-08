import React, { useEffect, useState } from 'react';
import { useSubagentStore, formatToolVerb, formatToolArg, type SubagentState, type ActiveToolCall } from '../../stores/subagents';

function formatElapsed(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  const remSecs = secs % 60;
  return `${mins}m${remSecs}s`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}


// ⚡ Bolt Performance Optimization:
// Isolate the high-frequency state update (1000ms interval) into a dedicated leaf component.
// This prevents `SubagentNode` and `ActiveToolRow` from re-rendering their entire subtrees every second.
const ElapsedTimeDisplay = React.memo(function ElapsedTimeDisplay({ startedAt, finished }: { startedAt: number; finished?: boolean }) {
  const [elapsed, setElapsed] = useState(() => Date.now() - startedAt);

  useEffect(() => {
    if (finished) return;
    const interval = setInterval(() => {
      setElapsed(Date.now() - startedAt);
    }, 1000);
    return () => clearInterval(interval);
  }, [startedAt, finished]);

  return <>{formatElapsed(elapsed)}</>;
});

const SPINNER_FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

function Spinner({ className }: { className?: string }) {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setFrame((f) => (f + 1) % SPINNER_FRAMES.length);
    }, 80);
    return () => clearInterval(interval);
  }, []);

  return <span className={className}>{SPINNER_FRAMES[frame]}</span>;
}

function ActiveToolRow({ tool, isLast }: { tool: ActiveToolCall; isLast: boolean }) {
  const verb = formatToolVerb(tool.toolName);
  const arg = formatToolArg(tool.toolName, tool.args);
  const connector = isLast ? '└─' : '├─';

  return (
    <div className="flex items-center gap-1.5 text-sm font-mono text-text-300 leading-6 pl-8">
      <span className="text-text-400">{connector}</span>
      <Spinner className="text-blue-400" />
      <span className="text-text-200">{verb}</span>
      {arg && <span className="text-text-400 truncate max-w-[300px]">{arg}</span>}
      <span className="text-text-400 ml-auto shrink-0">(<ElapsedTimeDisplay startedAt={tool.startedAt} />)</span>
    </div>
  );
}

function CompletedToolRow({ toolName, success, isLast }: { toolName: string; success: boolean; isLast: boolean }) {
  const connector = isLast ? '└─' : '├─';
  const icon = success ? '✓' : '✗';
  const color = success ? 'text-green-400' : 'text-red-400';

  return (
    <div className="flex items-center gap-1.5 text-sm font-mono text-text-400 leading-6 pl-8">
      <span className="text-text-400">{connector}</span>
      <span className={color}>{icon}</span>
      <span>{formatToolVerb(toolName)}</span>
    </div>
  );
}

function SubagentNode({ sa }: { sa: SubagentState }) {
  // Status indicator
  const statusEl = sa.finished ? (
    sa.success ? (
      <span className="text-green-400 font-bold">✓</span>
    ) : (
      <span className="text-red-400 font-bold">✗</span>
    )
  ) : (
    <Spinner className="text-blue-400" />
  );

  // Stats string
  const tokenStr = sa.tokenCount > 0 ? ` · ${formatTokens(sa.tokenCount)} tokens` : '';
  const statsPrefix = `(${sa.toolCallCount} tool uses${tokenStr} · `;
  const statsSuffix = `)`;

  // Display name
  const displayName = sa.name.split(/[-_]/).map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
  const taskPreview = sa.description.length > 60 ? sa.description.slice(0, 57) + '...' : sa.description;

  // Active tools
  const activeToolEntries = Array.from(sa.activeTools.values());
  // Show last 3 completed
  const completedVisible = sa.completedTools.slice(-3);
  const hiddenCount = Math.max(0, sa.toolCallCount - activeToolEntries.length - completedVisible.length);

  return (
    <div className="mb-1">
      {/* Header line */}
      <div className="flex items-center gap-1.5 text-sm font-mono leading-6">
        <span className="text-text-400 pl-2">├─</span>
        {statusEl}
        <span className="text-cyan-400 font-semibold">{displayName}</span>
        <span className="text-text-400 truncate">: {taskPreview}</span>
        <span className="text-text-400 ml-auto shrink-0 text-xs">{statsPrefix}<ElapsedTimeDisplay startedAt={sa.startedAt} finished={sa.finished} />{statsSuffix}</span>
      </div>

      {/* Active tool calls */}
      {activeToolEntries.map((tool, i) => (
        <ActiveToolRow
          key={tool.toolId}
          tool={tool}
          isLast={i === activeToolEntries.length - 1 && completedVisible.length === 0}
        />
      ))}

      {/* Last 3 completed tools */}
      {completedVisible.map((tool, i) => (
        <CompletedToolRow
          key={`completed-${i}`}
          toolName={tool.toolName}
          success={tool.success}
          isLast={i === completedVisible.length - 1}
        />
      ))}

      {/* Hidden count */}
      {hiddenCount > 0 && !sa.finished && (
        <div className="text-xs font-mono text-text-400 italic pl-10 leading-6">
          +{hiddenCount} more tool uses
        </div>
      )}

      {/* Shallow warning */}
      {sa.shallowWarning && (
        <div className="text-xs font-mono text-yellow-400 pl-10 leading-6">
          {sa.shallowWarning}
        </div>
      )}

      {/* Completion summary (persistent after finish) */}
      {sa.finished && (
        <div className="text-xs font-mono text-text-400 pl-10 leading-6">
          Done ({sa.toolCallCount} tool uses{tokenStr} · <ElapsedTimeDisplay startedAt={sa.startedAt} finished={sa.finished} />)
        </div>
      )}
    </div>
  );
}

export function SubagentTree() {
  const subagents = useSubagentStore((s) => s.subagents);
  const order = useSubagentStore((s) => s.order);

  if (order.length === 0) return null;

  // Only show if at least one subagent is not finished, or recently finished
  const activeSubagents = order.map(id => subagents.get(id)).filter(Boolean) as SubagentState[];
  if (activeSubagents.length === 0) return null;

  return (
    <div className="border-t border-border-300/30 bg-bg-100/30 py-2 px-2 shrink-0">
      <div className="text-xs font-mono text-text-300 font-semibold uppercase tracking-wide px-2 pb-1">
        Subagents
      </div>
      {activeSubagents.map((sa) => (
        <SubagentNode key={sa.subagentId} sa={sa} />
      ))}
    </div>
  );
}
