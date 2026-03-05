/**
 * Settings Modal with Vertical Sidebar Navigation
 *
 * Redesigned to use vertical tabs for better space utilization
 * and scalability as more settings categories are added.
 */

import { useState, useEffect } from 'react';
import { XMarkIcon } from '@heroicons/react/24/outline';
import {
  CpuChipIcon,
  ServerIcon,
  Cog6ToothIcon
} from '@heroicons/react/24/outline';
import { ModelSettings } from './ModelSettings';
import { MCPSettings } from './MCPSettings';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

type TabId = 'model' | 'mcp' | 'general';

interface TabConfig {
  id: TabId;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  description: string;
}

const tabs: TabConfig[] = [
  {
    id: 'model',
    label: 'Model',
    icon: CpuChipIcon,
    description: 'Configure AI model and provider settings'
  },
  {
    id: 'mcp',
    label: 'MCP Servers',
    icon: ServerIcon,
    description: 'Manage Model Context Protocol servers'
  },
  {
    id: 'general',
    label: 'General',
    icon: Cog6ToothIcon,
    description: 'General application settings'
  },
];

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
  const [activeTab, setActiveTab] = useState<TabId>('model');

  // Handle Escape key to close modal
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && isOpen) {
        onClose();
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const activeTabConfig = tabs.find(t => t.id === activeTab);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in">
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-6xl h-[85vh] flex flex-col overflow-hidden animate-slide-up">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200">
          <div>
            <h2 className="text-xl font-semibold text-gray-900">Settings</h2>
            {activeTabConfig && (
              <p className="text-xs text-gray-500 mt-0.5">{activeTabConfig.description}</p>
            )}
          </div>
          <button
            onClick={onClose}
            className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors"
          >
            <XMarkIcon className="w-5 h-5" />
          </button>
        </div>

        {/* Main Content Area with Sidebar */}
        <div className="flex-1 flex overflow-hidden">
          {/* Vertical Sidebar Navigation */}
          <div className="w-56 border-r border-gray-200 bg-gray-50 overflow-y-auto">
            <nav className="p-3 space-y-1">
              {tabs.map(tab => {
                const Icon = tab.icon;
                const isActive = activeTab === tab.id;

                return (
                  <button
                    key={tab.id}
                    onClick={() => setActiveTab(tab.id)}
                    className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors ${
                      isActive
                        ? 'bg-white text-gray-900 shadow-sm'
                        : 'text-gray-600 hover:bg-white/50 hover:text-gray-900'
                    }`}
                  >
                    <Icon className={`w-5 h-5 flex-shrink-0 ${isActive ? 'text-gray-900' : 'text-gray-400'}`} />
                    <span className="text-sm font-medium">{tab.label}</span>
                  </button>
                );
              })}
            </nav>

            {/* Sidebar Footer */}
            <div className="p-4 mt-4 border-t border-gray-200">
              <p className="text-xs text-gray-500">
                OpenDev v0.1.7
              </p>
            </div>
          </div>

          {/* Content Area */}
          <div className="flex-1 overflow-y-auto bg-white">
            <div className="p-6">
              {activeTab === 'model' && <ModelSettings />}
              {activeTab === 'mcp' && <MCPSettings />}
              {activeTab === 'general' && (
                <div className="text-center py-12">
                  <Cog6ToothIcon className="w-12 h-12 mx-auto text-gray-300 mb-3" />
                  <p className="text-sm text-gray-600 font-medium mb-1">General Settings</p>
                  <p className="text-xs text-gray-500">
                    General settings coming soon...
                  </p>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-gray-200 bg-gray-50">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium text-gray-700 hover:text-gray-900 hover:bg-white rounded-lg transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
