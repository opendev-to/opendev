/**
 * MCP Server Card Component
 *
 * Displays an MCP server with its status, configuration, and available actions.
 * Follows SRP by focusing solely on server presentation and user interactions.
 */

import { useState } from 'react';
import type { MCPServer } from '../../types/mcp';

interface MCPServerCardProps {
  server: MCPServer;
  onConnect: (name: string) => Promise<void>;
  onDisconnect: (name: string) => Promise<void>;
  onTest: (name: string) => Promise<void>;
  onViewTools: (name: string) => void;
  onEdit: (server: MCPServer) => void;
  onDelete: (name: string) => void;
}

export function MCPServerCard({
  server,
  onConnect,
  onDisconnect,
  onTest,
  onViewTools,
  onEdit,
  onDelete,
}: MCPServerCardProps) {
  const [isProcessing, setIsProcessing] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const handleConnectionToggle = async () => {
    setIsProcessing(true);
    try {
      if (server.status === 'connected') {
        await onDisconnect(server.name);
      } else {
        await onConnect(server.name);
      }
    } finally {
      setIsProcessing(false);
    }
  };

  const handleTest = async () => {
    setIsProcessing(true);
    try {
      await onTest(server.name);
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 hover:border-gray-300 transition-colors">
      {/* Header */}
      <div className="px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3 flex-1">
          <StatusIndicator status={server.status} isProcessing={isProcessing} />

          <div className="flex-1 min-w-0">
            <h4 className="text-sm font-medium text-gray-900 truncate">{server.name}</h4>
            <div className="flex items-center gap-2 mt-0.5">
              <span className="text-xs text-gray-500">
                {server.status === 'connected' ? `${server.tools_count} tools` : 'Not connected'}
              </span>
              <span className="text-xs text-gray-400">•</span>
              <span className="text-xs text-gray-500 capitalize">{server.config_location}</span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <ConnectionButton
            status={server.status}
            isProcessing={isProcessing}
            onClick={handleConnectionToggle}
          />

          <button
            onClick={() => setExpanded(!expanded)}
            className="p-1.5 text-gray-400 hover:text-gray-600 hover:bg-gray-50 rounded transition-colors"
          >
            <svg
              className={`w-4 h-4 transition-transform ${expanded ? 'rotate-180' : ''}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
            </svg>
          </button>
        </div>
      </div>

      {/* Expanded Details */}
      {expanded && (
        <div className="px-4 pb-3 border-t border-gray-100">
          <ServerDetails server={server} />

          <ActionButtons
            server={server}
            isProcessing={isProcessing}
            onTest={handleTest}
            onViewTools={onViewTools}
            onEdit={onEdit}
            onDelete={onDelete}
          />
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Sub-components (Single Responsibility Principle)
// ============================================================================

interface StatusIndicatorProps {
  status: MCPServer['status'];
  isProcessing: boolean;
}

function StatusIndicator({ status, isProcessing }: StatusIndicatorProps) {
  if (isProcessing) {
    return (
      <div className="flex items-center justify-center w-8 h-8">
        <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-900 rounded-full animate-spin" />
      </div>
    );
  }

  const statusConfig = {
    connected: { color: 'bg-green-500', label: 'Connected' },
    disconnected: { color: 'bg-gray-400', label: 'Disconnected' },
    connecting: { color: 'bg-yellow-500', label: 'Connecting' },
    error: { color: 'bg-red-500', label: 'Error' },
  };

  const config = statusConfig[status];

  return (
    <div className="relative flex items-center justify-center w-8 h-8">
      <div className={`w-2.5 h-2.5 rounded-full ${config.color}`} />
      {status === 'connected' && (
        <div className={`absolute w-2.5 h-2.5 rounded-full ${config.color} animate-ping opacity-75`} />
      )}
    </div>
  );
}

interface ConnectionButtonProps {
  status: MCPServer['status'];
  isProcessing: boolean;
  onClick: () => void;
}

function ConnectionButton({ status, isProcessing, onClick }: ConnectionButtonProps) {
  const isConnected = status === 'connected';

  return (
    <button
      onClick={onClick}
      disabled={isProcessing}
      className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors ${
        isConnected
          ? 'text-gray-700 bg-gray-100 hover:bg-gray-200'
          : 'text-white bg-gray-900 hover:bg-gray-800'
      } disabled:opacity-50 disabled:cursor-not-allowed`}
    >
      {isProcessing ? 'Processing...' : isConnected ? 'Disconnect' : 'Connect'}
    </button>
  );
}

interface ServerDetailsProps {
  server: MCPServer;
}

function ServerDetails({ server }: ServerDetailsProps) {
  const { config } = server;

  return (
    <div className="mt-3 space-y-2 text-xs">
      <DetailRow label="Command" value={config.command} mono />

      {config.args.length > 0 && (
        <DetailRow label="Args" value={config.args.join(' ')} mono />
      )}

      {Object.keys(config.env).length > 0 && (
        <div>
          <span className="text-gray-500 font-medium">Environment:</span>
          <div className="mt-1 space-y-1">
            {Object.entries(config.env).map(([key, value]) => (
              <div key={key} className="flex gap-2 text-gray-700">
                <span className="font-mono font-medium">{key}=</span>
                <span className="font-mono text-gray-600">{value}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="flex items-center gap-4">
        <DetailRow
          label="Auto-start"
          value={config.auto_start ? 'Enabled' : 'Disabled'}
          valueColor={config.auto_start ? 'text-green-600' : 'text-gray-500'}
        />
        <DetailRow
          label="Enabled"
          value={config.enabled ? 'Yes' : 'No'}
          valueColor={config.enabled ? 'text-green-600' : 'text-gray-500'}
        />
      </div>
    </div>
  );
}

interface DetailRowProps {
  label: string;
  value: string;
  mono?: boolean;
  valueColor?: string;
}

function DetailRow({ label, value, mono = false, valueColor = 'text-gray-700' }: DetailRowProps) {
  return (
    <div className="flex gap-2">
      <span className="text-gray-500 font-medium">{label}:</span>
      <span className={`${valueColor} ${mono ? 'font-mono' : ''} break-all`}>{value}</span>
    </div>
  );
}

interface ActionButtonsProps {
  server: MCPServer;
  isProcessing: boolean;
  onTest: () => void;
  onViewTools: (name: string) => void;
  onEdit: (server: MCPServer) => void;
  onDelete: (name: string) => void;
}

function ActionButtons({
  server,
  isProcessing,
  onTest,
  onViewTools,
  onEdit,
  onDelete,
}: ActionButtonsProps) {
  return (
    <div className="flex items-center gap-2 mt-3 pt-3 border-t border-gray-100">
      {server.status === 'connected' && (
        <ActionButton
          onClick={() => onViewTools(server.name)}
          disabled={isProcessing}
          variant="secondary"
        >
          View Tools ({server.tools_count})
        </ActionButton>
      )}

      <ActionButton
        onClick={onTest}
        disabled={isProcessing}
        variant="secondary"
      >
        Test Connection
      </ActionButton>

      <ActionButton
        onClick={() => onEdit(server)}
        disabled={isProcessing}
        variant="secondary"
      >
        Edit
      </ActionButton>

      <div className="flex-1" />

      <ActionButton
        onClick={() => onDelete(server.name)}
        disabled={isProcessing}
        variant="danger"
      >
        Remove
      </ActionButton>
    </div>
  );
}

interface ActionButtonProps {
  onClick: () => void;
  disabled: boolean;
  variant: 'primary' | 'secondary' | 'danger';
  children: React.ReactNode;
}

function ActionButton({ onClick, disabled, variant, children }: ActionButtonProps) {
  const variants = {
    primary: 'text-white bg-gray-900 hover:bg-gray-800',
    secondary: 'text-gray-700 bg-gray-100 hover:bg-gray-200',
    danger: 'text-red-700 bg-red-50 hover:bg-red-100',
  };

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors ${variants[variant]} disabled:opacity-50 disabled:cursor-not-allowed`}
    >
      {children}
    </button>
  );
}
