/**
 * Edit MCP Server Modal
 *
 * Modal for editing existing MCP server configurations.
 * Follows DRY by reusing form components from AddMCPServerModal.
 */

import { useState, useEffect } from 'react';
import { XMarkIcon } from '@heroicons/react/24/outline';
import type { MCPServer, MCPServerUpdateRequest } from '../../types/mcp';

interface EditMCPServerModalProps {
  isOpen: boolean;
  server: MCPServer | null;
  onClose: () => void;
  onSubmit: (name: string, update: MCPServerUpdateRequest) => Promise<void>;
}

interface FormData {
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  auto_start: boolean;
}

export function EditMCPServerModal({ isOpen, server, onClose, onSubmit }: EditMCPServerModalProps) {
  const [formData, setFormData] = useState<FormData | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Args management
  const [argInput, setArgInput] = useState('');

  // Env management
  const [envKey, setEnvKey] = useState('');
  const [envValue, setEnvValue] = useState('');

  // Initialize form data when server changes
  useEffect(() => {
    if (server) {
      setFormData({
        command: server.config.command,
        args: [...server.config.args],
        env: { ...server.config.env },
        enabled: server.config.enabled,
        auto_start: server.config.auto_start,
      });
    }
  }, [server]);

  if (!isOpen || !server || !formData) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!formData.command.trim()) {
      setError('Command is required');
      return;
    }

    setIsSubmitting(true);
    try {
      await onSubmit(server.name, {
        command: formData.command.trim(),
        args: formData.args.filter(arg => arg.trim()),
        env: formData.env,
        enabled: formData.enabled,
        auto_start: formData.auto_start,
      });

      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update server');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    if (!isSubmitting) {
      setArgInput('');
      setEnvKey('');
      setEnvValue('');
      setError(null);
      onClose();
    }
  };

  const addArg = () => {
    if (argInput.trim()) {
      setFormData(prev => prev ? ({
        ...prev,
        args: [...prev.args, argInput.trim()],
      }) : null);
      setArgInput('');
    }
  };

  const removeArg = (index: number) => {
    setFormData(prev => prev ? ({
      ...prev,
      args: prev.args.filter((_, i) => i !== index),
    }) : null);
  };

  const addEnvVar = () => {
    if (envKey.trim() && envValue.trim()) {
      setFormData(prev => prev ? ({
        ...prev,
        env: { ...prev.env, [envKey.trim()]: envValue.trim() },
      }) : null);
      setEnvKey('');
      setEnvValue('');
    }
  };

  const removeEnvVar = (key: string) => {
    setFormData(prev => {
      if (!prev) return null;
      const newEnv = { ...prev.env };
      delete newEnv[key];
      return { ...prev, env: newEnv };
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in">
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-2xl max-h-[85vh] flex flex-col overflow-hidden animate-slide-up">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200">
          <div>
            <h2 className="text-xl font-semibold text-gray-900">Edit MCP Server</h2>
            <p className="text-sm text-gray-500 mt-0.5">{server.name}</p>
          </div>
          <button
            type="button"
            onClick={handleClose}
            disabled={isSubmitting}
            className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors disabled:opacity-50"
          >
            <XMarkIcon className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <form onSubmit={handleSubmit} className="flex-1 overflow-y-auto p-6">
          <div className="space-y-4">
            {error && (
              <div className="px-4 py-3 bg-red-50 border border-red-200 rounded-lg">
                <p className="text-sm text-red-800">{error}</p>
              </div>
            )}

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Command <span className="text-red-500">*</span>
              </label>
              <input
                type="text"
                value={formData.command}
                onChange={(e) => setFormData(prev => prev ? ({ ...prev, command: e.target.value }) : null)}
                required
                disabled={isSubmitting}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent disabled:bg-gray-50 disabled:text-gray-500"
              />
            </div>

            {/* Arguments */}
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Arguments</label>
              <div className="space-y-2">
                {formData.args.map((arg, index) => (
                  <div key={index} className="flex items-center gap-2">
                    <input
                      type="text"
                      value={arg}
                      readOnly
                      className="flex-1 px-3 py-2 border border-gray-300 rounded-lg bg-gray-50 text-gray-700 font-mono text-sm"
                    />
                    <button
                      type="button"
                      onClick={() => removeArg(index)}
                      disabled={isSubmitting}
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
                    onChange={(e) => setArgInput(e.target.value)}
                    onKeyPress={(e) => e.key === 'Enter' && (e.preventDefault(), addArg())}
                    placeholder="Add argument..."
                    disabled={isSubmitting}
                    className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent disabled:bg-gray-50"
                  />
                  <button
                    type="button"
                    onClick={addArg}
                    disabled={isSubmitting || !argInput.trim()}
                    className="px-3 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-lg transition-colors disabled:opacity-50"
                  >
                    Add
                  </button>
                </div>
              </div>
            </div>

            {/* Environment Variables */}
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Environment Variables</label>
              <div className="space-y-2">
                {Object.entries(formData.env).map(([key, value]) => (
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
                      onClick={() => removeEnvVar(key)}
                      disabled={isSubmitting}
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
                    onChange={(e) => setEnvKey(e.target.value)}
                    placeholder="KEY"
                    disabled={isSubmitting}
                    className="w-32 px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent font-mono text-sm disabled:bg-gray-50"
                  />
                  <span className="text-gray-400">=</span>
                  <input
                    type="text"
                    value={envValue}
                    onChange={(e) => setEnvValue(e.target.value)}
                    onKeyPress={(e) => e.key === 'Enter' && (e.preventDefault(), addEnvVar())}
                    placeholder="value"
                    disabled={isSubmitting}
                    className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-gray-900 focus:border-transparent font-mono text-sm disabled:bg-gray-50"
                  />
                  <button
                    type="button"
                    onClick={addEnvVar}
                    disabled={isSubmitting || !envKey.trim() || !envValue.trim()}
                    className="px-3 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-lg transition-colors disabled:opacity-50"
                  >
                    Add
                  </button>
                </div>
              </div>
            </div>

            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={formData.auto_start}
                onChange={(e) => setFormData(prev => prev ? ({ ...prev, auto_start: e.target.checked }) : null)}
                disabled={isSubmitting}
                className="w-4 h-4 text-gray-900 border-gray-300 rounded focus:ring-gray-900 disabled:opacity-50"
              />
              <span className="text-sm text-gray-700">Enable auto-start on launch</span>
            </label>

            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={formData.enabled}
                onChange={(e) => setFormData(prev => prev ? ({ ...prev, enabled: e.target.checked }) : null)}
                disabled={isSubmitting}
                className="w-4 h-4 text-gray-900 border-gray-300 rounded focus:ring-gray-900 disabled:opacity-50"
              />
              <span className="text-sm text-gray-700">Enable this server</span>
            </label>
          </div>
        </form>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-gray-200 bg-gray-50">
          <button
            type="button"
            onClick={handleClose}
            disabled={isSubmitting}
            className="px-4 py-2 text-sm font-medium text-gray-700 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="submit"
            onClick={handleSubmit}
            disabled={isSubmitting}
            className="px-4 py-2 text-sm font-medium text-white bg-gray-900 hover:bg-gray-800 rounded-lg transition-colors disabled:opacity-50"
          >
            {isSubmitting ? 'Saving...' : 'Save Changes'}
          </button>
        </div>
      </div>
    </div>
  );
}
