import { useState } from 'react';
import type { Message } from '../../types';

interface ToolCallMessageProps {
  message: Message;
}

// Terminal-style tool display utilities
function getToolDisplayParts(toolName: string): { verb: string; label: string } {
  const toolMap: Record<string, { verb: string; label: string }> = {
    'read_file': { verb: 'Read', label: 'file' },
    'write_file': { verb: 'Write', label: 'file' },
    'edit_file': { verb: 'Edit', label: 'file' },
    'delete_file': { verb: 'Delete', label: 'file' },
    'list_files': { verb: 'List', label: 'files' },
    'list_directory': { verb: 'List', label: 'directory' },
    'search_code': { verb: 'Search', label: 'code' },
    'search': { verb: 'Search', label: 'project' },
    'run_command': { verb: 'Run', label: 'command' },
    'bash_execute': { verb: 'Run', label: 'command' },
    'fetch_url': { verb: 'Fetch', label: 'url' },
    'open_browser': { verb: 'Open', label: 'browser' },
    'capture_screenshot': { verb: 'Capture', label: 'screenshot' },
    'analyze_image': { verb: 'Analyze', label: 'image' },
    'git_commit': { verb: 'Commit', label: 'changes' },
    'present_plan': { verb: 'Present', label: 'plan' },
    'spawn_subagent': { verb: 'Spawn', label: 'agent' },
    'task_complete': { verb: 'Complete', label: 'task' },
    'invoke_skill': { verb: 'Invoke', label: 'skill' },
  };

  if (toolName.startsWith('mcp__')) {
    const parts = toolName.split('__', 2);
    if (parts.length === 3) {
      return { verb: 'MCP', label: `${parts[1]}/${parts[2]}` };
    }
    if (parts.length === 2) {
      return { verb: 'MCP', label: parts[1] };
    }
    return { verb: 'MCP', label: 'tool' };
  }

  return toolMap[toolName] || { verb: 'Call', label: 'tool' };
}

function summarizeToolArgs(toolName: string, toolArgs: any): string {
  if (!toolArgs || typeof toolArgs !== 'object') return '';

  const primaryKeys: Record<string, string[]> = {
    'read_file': ['file_path', 'path'],
    'write_file': ['file_path', 'path'],
    'edit_file': ['file_path', 'path'],
    'delete_file': ['file_path', 'path'],
    'list_files': ['path', 'directory'],
    'list_directory': ['path', 'directory'],
    'search_code': ['pattern', 'query'],
    'search': ['query'],
    'run_command': ['command'],
    'bash_execute': ['command'],
    'fetch_url': ['url'],
    'open_browser': ['url'],
    'capture_screenshot': ['target', 'path'],
    'capture_web_screenshot': ['url'],
    'analyze_image': ['image_path', 'file_path'],
    'git_commit': ['message'],
    'present_plan': ['plan_file_path'],
    'spawn_subagent': ['agent_type', 'description'],
    'task_complete': ['summary'],
    'invoke_skill': ['skill_name'],
  };

  const keys = primaryKeys[toolName] || Object.keys(toolArgs);
  for (const key of keys) {
    if (toolArgs[key] && typeof toolArgs[key] === 'string') {
      return toolArgs[key];
    }
  }
  return '';
}

// Tool result summarization functions (based on StyleFormatter from terminal UI)
function formatToolResult(toolName: string, toolArgs: any, result: any): string[] {
  if (result?.success === false) {
    const errorMsg = result?.error || 'Unknown error';
    if (errorMsg.toLowerCase().includes('interrupted')) {
      return ['User interrupted'];
    }
    return [`Error: ${errorMsg}`];
  }

  if (toolName === 'read_file') {
    return formatReadFileResult(toolArgs, result);
  } else if (toolName === 'write_file') {
    return formatWriteFileResult(toolArgs, result);
  } else if (toolName === 'edit_file') {
    return formatEditFileResult(toolArgs, result);
  } else if (toolName === 'search' || toolName === 'search_code') {
    return formatSearchResult(toolArgs, result);
  } else if (toolName === 'run_command' || toolName === 'bash_execute') {
    return formatShellResult(toolArgs, result);
  } else if (toolName === 'list_files') {
    return formatListFilesResult(toolArgs, result);
  } else if (toolName === 'fetch_url') {
    return formatFetchUrlResult(toolArgs, result);
  } else if (toolName === 'present_plan') {
    return formatPresentPlanResult(toolArgs, result);
  } else if (toolName === 'spawn_subagent') {
    return formatSpawnSubagentResult(toolArgs, result);
  } else if (toolName === 'task_complete') {
    return formatTaskCompleteResult(toolArgs, result);
  } else {
    return formatGenericResult(toolArgs, result);
  }
}

function formatReadFileResult(_toolArgs: any, result: any): string[] {
  const output = result?.output || result?.content || '';
  const sizeBytes = output.length;
  const sizeKb = sizeBytes / 1024;
  const lines = output ? output.split('\n').length : 0;

  const sizeDisplay = sizeKb >= 1 ? `${sizeKb.toFixed(1)} KB` : `${sizeBytes} B`;
  return [`Read ${lines} lines • ${sizeDisplay}`];
}

function formatWriteFileResult(toolArgs: any, _result: any): string[] {
  const filePath = toolArgs?.file_path || toolArgs?.path || 'unknown';
  const content = toolArgs?.content || '';
  const sizeBytes = content.length;
  const sizeKb = sizeBytes / 1024;
  const lines = content ? content.split('\n').length : 0;

  const fileName = filePath.split('/').pop() || filePath;
  const sizeDisplay = sizeKb >= 1 ? `${sizeKb.toFixed(1)} KB` : `${sizeBytes} B`;
  return [`Created ${fileName} • ${sizeDisplay} • ${lines} lines`];
}

function formatEditFileResult(toolArgs: any, _result: any): string[] {
  const filePath = toolArgs?.file_path || toolArgs?.path || 'unknown';
  const fileName = filePath.split('/').pop() || filePath;
  return [`Updated ${fileName}`];
}

function formatSearchResult(_toolArgs: any, result: any): string[] {
  const matches = result?.matches || [];
  const output = result?.output || '';

  if (matches.length > 0) {
    const summary = matches.slice(0, 3).map((match: any) => {
      const line = typeof match === 'string' ? match : match.line || match.content || '';
      const preview = line.length > 50 ? line.slice(0, 47) + '...' : line;
      return preview;
    });

    if (matches.length > 3) {
      summary.push(`... and ${matches.length - 3} more`);
    }
    return summary;
  }

  if (output) {
    const lines = output.split('\n');
    return lines.slice(0, 3);
  }

  return ['No matches found'];
}

function formatShellResult(toolArgs: any, result: any): string[] {
  const command = toolArgs?.command || '';
  const stdout = result?.stdout || result?.output || '';
  const stderr = result?.stderr || '';
  const exitCode = result?.exit_code;

  if (exitCode !== undefined && exitCode !== 0 && exitCode !== null) {
    return stderr ? [stderr.split('\n')[0]] : [`Exit code ${exitCode}`];
  }

  const normalizedCmd = command.toLowerCase();
  const normalizedStdout = stdout.toLowerCase();

  // Special git command handling
  if (normalizedCmd.includes('git ')) {
    if (normalizedCmd.includes('push')) return ['Changes pushed to remote'];
    if (normalizedCmd.includes('commit')) return ['Changes committed'];
    if (normalizedCmd.includes('pull')) return ['Changes pulled from remote'];
    return ['Git command completed'];
  }

  // Special npm command handling
  if (normalizedCmd.includes('npm install')) {
    if (normalizedStdout.includes('added') && normalizedStdout.includes('package')) {
      return ['Packages installed successfully'];
    }
    return ['npm install completed'];
  }

  if (stdout) {
    const lines = stdout.split('\n').filter((line: string) => line.trim());
    if (lines.length === 1 && lines[0].length < 80) {
      return [lines[0]];
    }
    const firstLine = lines[0];
    const preview = firstLine.length > 70 ? firstLine.slice(0, 70) + '...' : firstLine;
    return [`${preview} (${lines.length} lines)`];
  }

  if (stderr) {
    return [stderr.split('\n')[0]];
  }

  return ['Command completed with no output'];
}

function formatListFilesResult(_toolArgs: any, result: any): string[] {
  const entries = result?.entries;
  if (entries && Array.isArray(entries)) {
    return [`${entries.length} entries`];
  }

  const output = result?.output || '';
  if (!output) {
    return ['No files found'];
  }

  const lines = output.split('\n').filter((line: string) => line.trim());
  return lines.length > 0 ? [`${lines.length} items`] : ['No files found'];
}

function formatFetchUrlResult(_toolArgs: any, result: any): string[] {
  const status = result?.status_code || result?.status;
  const output = result?.output || '';

  if (status) {
    return [`Status ${status} • ${output.length} bytes`];
  }

  return [`${output.length} bytes received`];
}

function formatPresentPlanResult(_toolArgs: any, result: any): string[] {
  if (result?.plan_approved) {
    return result?.auto_approve
      ? ['Plan approved (auto-approve edits)']
      : ['Plan approved (review edits)'];
  }
  if (result?.requires_modification) {
    return ['Plan requires modifications'];
  }
  if (result?.plan_rejected) {
    return ['Plan rejected'];
  }
  return [result?.output || 'Plan presented'];
}

function formatSpawnSubagentResult(toolArgs: any, result: any): string[] {
  const agentType = toolArgs?.agent_type || 'unknown';
  const output = result?.output || '';
  if (result?.success === false) {
    return [`${agentType} agent failed`];
  }
  if (output) {
    const firstLine = output.split('\n')[0];
    return [firstLine.length > 80 ? firstLine.slice(0, 77) + '...' : firstLine];
  }
  return [`${agentType} agent completed`];
}

function formatTaskCompleteResult(_toolArgs: any, result: any): string[] {
  const summary = result?.output || result?.summary || '';
  if (summary) {
    return [summary.length > 100 ? summary.slice(0, 97) + '...' : summary];
  }
  return ['Task completed'];
}

function formatGenericResult(_toolArgs: any, result: any): string[] {
  const output = result?.output || '';

  if (typeof output === 'string') {
    const lines = output.split('\n').filter((line: string) => line.trim());
    if (lines.length === 0) return [];
    return lines.slice(0, 3).concat(lines.length > 3 ? ['…'] : []);
  }

  if (Array.isArray(output)) {
    const summary = output.slice(0, 3).map((item: any) => String(item));
    if (output.length > 3) {
      summary.push('…');
    }
    return summary;
  }

  if (output && typeof output === 'object') {
    return ['Object received'];
  }

  return output ? [String(output)] : [];
}

export function ToolCallMessage({ message }: ToolCallMessageProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  if (message.role === 'tool_call') {
    const toolName = message.tool_name ||
                     (message.tool_calls && message.tool_calls[0]?.name) ||
                     (message as any)?.name || '';

    const toolArgs = message.tool_args || {};
    const toolResult = message.tool_result ?? {};
    const summaryOverride = message.tool_summary;
    const successOverride = message.tool_success;

    const { verb } = getToolDisplayParts(toolName);
    const summary =
      message.tool_args_display ??
      summarizeToolArgs(toolName, toolArgs);

    // Handle result processing
    let resultData = toolResult;
    if (typeof toolResult === 'string') {
      try {
        resultData = JSON.parse(toolResult);
      } catch {
        resultData = {
          output: toolResult,
          success: !toolResult.includes('::tool_error::') && !toolResult.includes('::interrupted::')
        };
      }
    }


    // Get result summary
    let summaryLines: string[] = [];
    if (summaryOverride) {
      summaryLines = Array.isArray(summaryOverride)
        ? summaryOverride
        : [summaryOverride];
    } else {
      try {
        summaryLines = formatToolResult(toolName, toolArgs, resultData);
      } catch {
        if (typeof toolResult === 'string') {
          const cleaned = toolResult.replace(/::tool_error::|::interrupted::/g, '').trim();
          summaryLines = cleaned.split('\n').slice(0, 3).filter((line: string) => line.trim());
          if (cleaned.split('\n').length > 3) {
            summaryLines.push('…');
          }
        } else {
          summaryLines = ['Tool completed'];
        }
      }
    }

    // Check for expandable content
    let fullOutput: string | undefined;
    if (typeof toolResult === 'string') {
      fullOutput = toolResult;
    } else if (resultData?.output) {
      fullOutput = typeof resultData.output === 'string'
        ? resultData.output
        : JSON.stringify(resultData.output, null, 2);
    } else if (Object.keys(resultData || {}).length > 0) {
      try {
        fullOutput = JSON.stringify(resultData, null, 2);
      } catch {
        fullOutput = String(resultData);
      }
    }
    const hasExpandableContent = !!fullOutput && fullOutput.length > 200;

    return (
      <div className="bg-bg-100 border border-border-300/15 rounded-lg px-4 py-3">
        {/* Tool action header */}
        <div className="flex items-center gap-2 mb-2">
          <span className="text-text-400 font-mono text-sm leading-6 flex-shrink-0">▶</span>
          <span className="font-medium text-text-000 text-sm leading-6">
            {verb}
          </span>
          {summary && (
            <span className="text-text-200 text-sm bg-bg-000 px-2 py-1 rounded border border-border-300/20 font-mono leading-6">
              {summary}
            </span>
          )}
        </div>

        {/* Tool result summary with proper colors */}
        {summaryLines.length > 0 && (
          <div className="ml-4 pl-3 border-l-2 border-border-300/30">
            {summaryLines.map((line: string, index: number) => {
              // Check if this line indicates success or failure
              const isSuccess = successOverride ?? (
                line.includes('Read') || line.includes('Created') || line.includes('Updated') ||
                line.includes('Changes') || line.includes('Packages installed') || line.includes('completed')
              );
              const isError = message.tool_error
                ? true
                : line.includes('Error') || line.includes('Failed') || line.includes('interrupted') || line.includes('Exit code');

              return (
                <div key={index} className={`font-mono text-sm mb-1 leading-6 ${
                  isError ? 'text-danger-100' :
                  isSuccess ? 'text-success-100' :
                  'text-text-300'
                }`}>
                  {line}
                </div>
              );
            })}
          </div>
        )}

        {/* Expand button */}
        {hasExpandableContent && (
          <button
            onClick={() => setIsExpanded(!isExpanded)}
            className="ml-4 text-sm text-text-400 hover:text-text-200 font-medium mt-2 leading-6"
          >
            {isExpanded ? 'Hide details' : 'Show details'}
          </button>
        )}

        {/* Expanded content */}
        {hasExpandableContent && isExpanded && (
          <div className="ml-4 mt-3 pl-3 border-t border-border-300/15 pt-3">
            {fullOutput && (
              <pre className="text-sm text-text-300 font-mono bg-bg-000 border border-border-300/15 rounded p-3 overflow-x-auto max-h-96 leading-6">
                {fullOutput}
              </pre>
            )}
          </div>
        )}
      </div>
    );
  }

  return null;
}
