/**
 * Add MCP Server Modal
 *
 * Modal for adding new MCP server configurations.
 * Supports both manual form entry and JSON import.
 */

import { useState } from 'react';
import { XMarkIcon } from '@heroicons/react/24/outline';
import type { MCPServerCreateRequest } from '../../types/mcp';

interface AddMCPServerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSubmit: (server: MCPServerCreateRequest) => Promise<void>;
}

interface FormData {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  auto_start: boolean;
  project_config: boolean;
}

const initialFormData: FormData = {
  name: '',
  command: '',
  args: [],
  env: {},
  enabled: true,
  auto_start: false,
  project_config: false,
};

type InputMode = 'form' | 'json';

export function AddMCPServerModal({ isOpen, onClose, onSubmit }: AddMCPServerModalProps) {
  const [mode, setMode] = useState<InputMode>('form');
  const [formData, setFormData] = useState<FormData>(initialFormData);
  const [jsonInput, setJsonInput] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Args management
  const [argInput, setArgInput] = useState('');

  // Env management
  const [envKey, setEnvKey] = useState('');
  const [envValue, setEnvValue] = useState('');

  if (!isOpen) return null;

  const parseJSON = () => {
    try {
      const parsed = JSON.parse(jsonInput);

      // Support Claude Code format: { "mcpServers": { "name": { config } } }
      if (parsed.mcpServers) {
        const serverName = Object.keys(parsed.mcpServers)[0];
        if (!serverName) {
          setError('No server found in JSON');
          return;
        }
        const serverConfig = parsed.mcpServers[serverName];
        setFormData({
          name: serverName,
          command: serverConfig.command || '',
          args: serverConfig.args || [],
          env: serverConfig.env || {},
          enabled: serverConfig.enabled ?? true,
          auto_start: serverConfig.auto_start ?? false,
          project_config: false,
        });
        setMode('form');
        setError(null);
      }
      // Support direct server config format: { "command": "...", "args": [...] }
      else if (parsed.command) {
        setFormData({
          name: parsed.name || '',
          command: parsed.command,
          args: parsed.args || [],
          env: parsed.env || {},
          enabled: parsed.enabled ?? true,
          auto_start: parsed.auto_start ?? false,
          project_config: parsed.project_config ?? false,
        });
        setMode('form');
        setError(null);
      } else {
        setError('Invalid JSON format. Expected either Claude Code format or server config.');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid JSON');
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    // Validation
    if (!formData.name.trim()) {
      setError('Server name is required');
      return;
    }

    if (!formData.command.trim()) {
      setError('Command is required');
      return;
    }

    setIsSubmitting(true);
    try {
      await onSubmit({
        name: formData.name.trim(),
        command: formData.command.trim(),
        args: formData.args.filter(arg => arg.trim()),
        env: formData.env,
        enabled: formData.enabled,
        auto_start: formData.auto_start,
        project_config: formData.project_config,
      });

      // Reset form and close
      setFormData(initialFormData);
      setJsonInput('');
      setArgInput('');
      setEnvKey('');
      setEnvValue('');
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to add server');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    if (!isSubmitting) {
      setFormData(initialFormData);
      setJsonInput('');
      setArgInput('');
      setEnvKey('');
      setEnvValue('');
      setError(null);
      setMode('form');
      onClose();
    }
  };

  const addArg = () => {
    if (argInput.trim()) {
      setFormData(prev => ({
        ...prev,
        args: [...prev.args, argInput.trim()],
      }));
      setArgInput('');
    }
  };

  const removeArg = (index: number) => {
    setFormData(prev => ({
      ...prev,
      args: prev.args.filter((_, i) => i !== index),
    }));
  };

  const addEnvVar = () => {
    if (envKey.trim() && envValue.trim()) {
      setFormData(prev => ({
        ...prev,
        env: { ...prev.env, [envKey.trim()]: envValue.trim() },
      }));
      setEnvKey('');
      setEnvValue('');
    }
  };

  const removeEnvVar = (key: string) => {
    setFormData(prev => {
      const newEnv = { ...prev.env };
      delete newEnv[key];
      return { ...prev, env: newEnv };
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in">
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-2xl max-h-[85vh] flex flex-col overflow-hidden animate-slide-up">
        {/* Header */}
        <ModalHeader title="Add MCP Server" onClose={handleClose} disabled={isSubmitting} />

        {/* Mode Tabs */}
        <div className="flex border-b border-gray-200 px-6">
          <button
            type="button"
            onClick={() => setMode('form')}
            disabled={isSubmitting}
            className={`px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
              mode === 'form'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-500 hover:text-gray-700'
            }`}
          >
            Manual Entry
          </button>
          <button
            type="button"
            onClick={() => setMode('json')}
            disabled={isSubmitting}
            className={`px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
              mode === 'json'
                ? 'border-gray-900 text-gray-900'
                : 'border-transparent text-gray-500 hover:text-gray-700'
            }`}
          >
            Import JSON
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {error && <ErrorMessage message={error} />}

          {mode === 'json' ? (
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Paste JSON Configuration
                </label>
                <p className="text-xs text-gray-500 mb-3">
                  Paste your MCP server JSON from Claude Code format or direct server config
                </p>
                <textarea
                  value={jsonInput}
                  onChange={(e) => setJsonInput(e.target.value)}
                  placeholder={`{\n  "mcpServers": {\n    "server-name": {\n      "command": "npx",\n      "args": ["-y", "package-name"]\n    }\n  }\n}`}
                  disabled={isSubmitting}
                  rows={12}
                  className="w-full px-3 py-2 border border-gray-300 rounded-lg font-mono text-sm focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent disabled:bg-gray-50"
                />
              </div>
              <button
                type="button"
                onClick={parseJSON}
                disabled={isSubmitting || !jsonInput.trim()}
                className="px-4 py-2 text-sm font-medium text-white bg-gray-900 hover:bg-gray-800 rounded-lg transition-colors disabled:opacity-50"
              >
                Parse and Fill Form
              </button>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4">
              <TextField
                label="Server Name"
                value={formData.name}
                onChange={(value) => setFormData(prev => ({ ...prev, name: value }))}
                placeholder="e.g., github, filesystem"
                required
                disabled={isSubmitting}
              />

              <TextField
                label="Command"
                value={formData.command}
                onChange={(value) => setFormData(prev => ({ ...prev, command: value }))}
                placeholder="e.g., npx -y @modelcontextprotocol/server-github"
                required
                disabled={isSubmitting}
              />

              <ArgumentsList
                args={formData.args}
                argInput={argInput}
                onArgInputChange={setArgInput}
                onAddArg={addArg}
                onRemoveArg={removeArg}
                disabled={isSubmitting}
              />

              <EnvironmentVariables
                env={formData.env}
                envKey={envKey}
                envValue={envValue}
                onEnvKeyChange={setEnvKey}
                onEnvValueChange={setEnvValue}
                onAddEnv={addEnvVar}
                onRemoveEnv={removeEnvVar}
                disabled={isSubmitting}
              />

              <CheckboxField
                label="Enable auto-start on launch"
                checked={formData.auto_start}
                onChange={(checked) => setFormData(prev => ({ ...prev, auto_start: checked }))}
                disabled={isSubmitting}
              />

              <CheckboxField
                label="Enable this server"
                checked={formData.enabled}
                onChange={(checked) => setFormData(prev => ({ ...prev, enabled: checked }))}
                disabled={isSubmitting}
              />

              <CheckboxField
                label="Save to project config (instead of global)"
                checked={formData.project_config}
                onChange={(checked) => setFormData(prev => ({ ...prev, project_config: checked }))}
                disabled={isSubmitting}
              />
            </form>
          )}
        </div>

        {/* Footer */}
        <ModalFooter
          onClose={handleClose}
          onSubmit={handleSubmit}
          isSubmitting={isSubmitting}
          submitLabel="Add Server"
          showSubmit={mode === 'form'}
        />
      </div>
    </div>
  );
}

// ============================================================================
// Sub-components
// ============================================================================

interface ModalHeaderProps {
  title: string;
  onClose: () => void;
  disabled: boolean;
}

function ModalHeader({ title, onClose, disabled }: ModalHeaderProps) {
  return (
    <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200">
      <h2 className="text-xl font-semibold text-gray-900">{title}</h2>
      <button
        type="button"
        onClick={onClose}
        disabled={disabled}
        className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors disabled:opacity-50"
      >
        <XMarkIcon className="w-5 h-5" />
      </button>
    </div>
  );
}

interface ModalFooterProps {
  onClose: () => void;
  onSubmit: (e: React.FormEvent) => void;
  isSubmitting: boolean;
  submitLabel: string;
  showSubmit?: boolean;
}

function ModalFooter({ onClose, onSubmit, isSubmitting, submitLabel, showSubmit = true }: ModalFooterProps) {
  return (
    <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-gray-200 bg-gray-50">
      <button
        type="button"
        onClick={onClose}
        disabled={isSubmitting}
        className="px-4 py-2 text-sm font-medium text-gray-700 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors disabled:opacity-50"
      >
        Cancel
      </button>
      {showSubmit && (
        <button
          type="submit"
          onClick={onSubmit}
          disabled={isSubmitting}
          className="px-4 py-2 text-sm font-medium text-white bg-gray-900 hover:bg-gray-800 rounded-lg transition-colors disabled:opacity-50"
        >
          {isSubmitting ? 'Adding...' : submitLabel}
        </button>
      )}
    </div>
  );
}

interface ErrorMessageProps {
  message: string;
}

function ErrorMessage({ message }: ErrorMessageProps) {
  return (
    <div className="px-4 py-3 bg-red-50 border border-red-200 rounded-lg mb-4">
      <p className="text-sm text-red-800">{message}</p>
    </div>
  );
}

interface TextFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  required?: boolean;
  disabled?: boolean;
}

function TextField({ label, value, onChange, placeholder, required, disabled }: TextFieldProps) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 mb-1">
        {label}
        {required && <span className="text-red-500 ml-1">*</span>}
      </label>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        required={required}
        disabled={disabled}
        className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent disabled:bg-gray-50 disabled:text-gray-500"
      />
    </div>
  );
}

interface CheckboxFieldProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}

function CheckboxField({ label, checked, onChange, disabled }: CheckboxFieldProps) {
  return (
    <label className="flex items-center gap-2 cursor-pointer">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        disabled={disabled}
        className="w-4 h-4 text-gray-900 border-gray-300 rounded focus:ring-gray-900 disabled:opacity-50"
      />
      <span className="text-sm text-gray-700">{label}</span>
    </label>
  );
}

interface ArgumentsListProps {
  args: string[];
  argInput: string;
  onArgInputChange: (value: string) => void;
  onAddArg: () => void;
  onRemoveArg: (index: number) => void;
  disabled?: boolean;
}

function ArgumentsList({
  args,
  argInput,
  onArgInputChange,
  onAddArg,
  onRemoveArg,
  disabled,
}: ArgumentsListProps) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 mb-1">Arguments</label>
      <div className="space-y-2">
        {args.map((arg, index) => (
          <div key={index} className="flex items-center gap-2">
            <input
              type="text"
              value={arg}
              readOnly
              className="flex-1 px-3 py-2 border border-gray-300 rounded-lg bg-gray-50 text-gray-700 font-mono text-sm"
            />
            <button
              type="button"
              onClick={() => onRemoveArg(index)}
              disabled={disabled}
              className="p-2 text-red-600 hover:bg-red-50 rounded-lg transition-colors disabled:opacity-50"
            >
              <XMarkIcon className="w-4 h-4" />
            </button>
          </div>
        ))}
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={argInput}
            onChange={(e) => onArgInputChange(e.target.value)}
            onKeyPress={(e) => e.key === 'Enter' && (e.preventDefault(), onAddArg())}
            placeholder="Add argument..."
            disabled={disabled}
            className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent disabled:bg-gray-50"
          />
          <button
            type="button"
            onClick={onAddArg}
            disabled={disabled || !argInput.trim()}
            className="px-3 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-lg transition-colors disabled:opacity-50"
          >
            Add
          </button>
        </div>
      </div>
    </div>
  );
}

interface EnvironmentVariablesProps {
  env: Record<string, string>;
  envKey: string;
  envValue: string;
  onEnvKeyChange: (value: string) => void;
  onEnvValueChange: (value: string) => void;
  onAddEnv: () => void;
  onRemoveEnv: (key: string) => void;
  disabled?: boolean;
}

function EnvironmentVariables({
  env,
  envKey,
  envValue,
  onEnvKeyChange,
  onEnvValueChange,
  onAddEnv,
  onRemoveEnv,
  disabled,
}: EnvironmentVariablesProps) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 mb-1">Environment Variables</label>
      <div className="space-y-2">
        {Object.entries(env).map(([key, value]) => (
          <div key={key} className="flex items-center gap-2">
            <span className="px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm font-mono text-gray-700">
              {key}
            </span>
            <span className="text-gray-400">=</span>
            <input
              type="text"
              value={value}
              readOnly
              className="flex-1 px-3 py-2 border border-gray-300 rounded-lg bg-gray-50 text-gray-700 font-mono text-sm"
            />
            <button
              type="button"
              onClick={() => onRemoveEnv(key)}
              disabled={disabled}
              className="p-2 text-red-600 hover:bg-red-50 rounded-lg transition-colors disabled:opacity-50"
            >
              <XMarkIcon className="w-4 h-4" />
            </button>
          </div>
        ))}
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={envKey}
            onChange={(e) => onEnvKeyChange(e.target.value)}
            placeholder="KEY"
            disabled={disabled}
            className="w-32 px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent font-mono text-sm disabled:bg-gray-50"
          />
          <span className="text-gray-400">=</span>
          <input
            type="text"
            value={envValue}
            onChange={(e) => onEnvValueChange(e.target.value)}
            onKeyPress={(e) => e.key === 'Enter' && (e.preventDefault(), onAddEnv())}
            placeholder="value"
            disabled={disabled}
            className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent font-mono text-sm disabled:bg-gray-50"
          />
          <button
            type="button"
            onClick={onAddEnv}
            disabled={disabled || !envKey.trim() || !envValue.trim()}
            className="px-3 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-lg transition-colors disabled:opacity-50"
          >
            Add
          </button>
        </div>
      </div>
    </div>
  );
}
