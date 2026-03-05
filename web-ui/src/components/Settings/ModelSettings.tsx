import { useState, useEffect } from 'react';
import { apiClient } from '../../api/client';

interface Provider {
  id: string;
  name: string;
  description: string;
  models: Model[];
}

interface Model {
  id: string;
  name: string;
  description: string;
}

interface Config {
  model_provider: string;
  model: string;
  model_thinking_provider?: string | null;
  model_thinking?: string | null;
  model_vlm_provider?: string | null;
  model_vlm?: string | null;
  model_critique_provider?: string | null;
  model_critique?: string | null;
  model_compact_provider?: string | null;
  model_compact?: string | null;
  temperature: number;
  max_tokens: number;
}

interface ModelSlotProps {
  title: string;
  description: string;
  icon: React.ReactNode;
  providers: Provider[];
  selectedProvider: string;
  selectedModel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
  optional?: boolean;
  notSetText?: string;
}

function ModelSlot({
  title,
  description,
  icon,
  providers,
  selectedProvider,
  selectedModel,
  onProviderChange,
  onModelChange,
  optional = false,
  notSetText = "Not configured"
}: ModelSlotProps) {
  const currentProvider = providers.find(p => p.id === selectedProvider);
  const availableModels = currentProvider?.models || [];

  return (
    <div className="border border-gray-200 rounded-lg p-4 bg-gradient-to-br from-white to-gray-50">
      {/* Header */}
      <div className="flex items-center gap-3 mb-4">
        <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center shadow-md flex-shrink-0">
          {icon}
        </div>
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <h3 className="text-base font-semibold text-gray-900">{title}</h3>
            {optional && (
              <span className="text-xs px-2 py-0.5 bg-gray-200 text-gray-600 rounded-full">
                Optional
              </span>
            )}
          </div>
          <p className="text-xs text-gray-600 mt-0.5">{description}</p>
        </div>
      </div>

      {/* Provider Selection */}
      <div className="space-y-3">
        <div>
          <label className="block text-xs font-medium text-gray-700 mb-1.5">
            Provider
          </label>
          <select
            value={selectedProvider || ''}
            onChange={(e) => {
              const newProvider = e.target.value;
              onProviderChange(newProvider);
              // Reset model selection when provider changes
              const provider = providers.find(p => p.id === newProvider);
              if (provider && provider.models.length > 0) {
                onModelChange(provider.models[0].id);
              }
            }}
            className="w-full px-3 py-2 text-sm border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent bg-white"
          >
            {optional && (
              <option value="">{notSetText}</option>
            )}
            {providers.map(provider => (
              <option key={provider.id} value={provider.id}>
                {provider.name}
              </option>
            ))}
          </select>
        </div>

        {/* Model Selection */}
        {selectedProvider && (
          <div>
            <label className="block text-xs font-medium text-gray-700 mb-1.5">
              Model
            </label>
            <select
              value={selectedModel || ''}
              onChange={(e) => onModelChange(e.target.value)}
              className="w-full px-3 py-2 text-sm border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent bg-white"
              disabled={availableModels.length === 0}
            >
              {availableModels.map(model => (
                <option key={model.id} value={model.id}>
                  {model.name}
                </option>
              ))}
            </select>
            {availableModels.find(m => m.id === selectedModel) && (
              <p className="mt-1.5 text-xs text-gray-500">
                {availableModels.find(m => m.id === selectedModel)?.description}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export function ModelSettings() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [_config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);

  // Normal model
  const [normalProvider, setNormalProvider] = useState<string>('');
  const [normalModel, setNormalModel] = useState<string>('');

  // Thinking model
  const [thinkingProvider, setThinkingProvider] = useState<string>('');
  const [thinkingModel, setThinkingModel] = useState<string>('');

  // Vision model
  const [visionProvider, setVisionProvider] = useState<string>('');
  const [visionModel, setVisionModel] = useState<string>('');

  // Critique model
  const [critiqueProvider, setCritiqueProvider] = useState<string>('');
  const [critiqueModel, setCritiqueModel] = useState<string>('');

  // Compact model
  const [compactProvider, setCompactProvider] = useState<string>('');
  const [compactModel, setCompactModel] = useState<string>('');

  // Other settings
  const [temperature, setTemperature] = useState<number>(0.7);
  const [maxTokens, setMaxTokens] = useState<number>(4096);

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      setLoading(true);
      const [providersData, configData] = await Promise.all([
        apiClient.listProviders(),
        apiClient.getConfig(),
      ]);

      setProviders(providersData);
      setConfig(configData);

      // Normal model
      setNormalProvider(configData.model_provider);
      setNormalModel(configData.model);

      // Thinking model
      setThinkingProvider(configData.model_thinking_provider || '');
      setThinkingModel(configData.model_thinking || '');

      // Vision model
      setVisionProvider(configData.model_vlm_provider || '');
      setVisionModel(configData.model_vlm || '');

      // Critique model
      setCritiqueProvider(configData.model_critique_provider || '');
      setCritiqueModel(configData.model_critique || '');

      // Compact model
      setCompactProvider(configData.model_compact_provider || '');
      setCompactModel(configData.model_compact || '');

      // Other settings
      setTemperature(configData.temperature);
      setMaxTokens(configData.max_tokens);
    } catch (error) {
      console.error('Failed to load settings:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    try {
      setSaving(true);
      setSaveSuccess(false);

      await apiClient.updateConfig({
        model_provider: normalProvider,
        model: normalModel,
        model_thinking_provider: thinkingProvider || null,
        model_thinking: thinkingModel || null,
        model_vlm_provider: visionProvider || null,
        model_vlm: visionModel || null,
        model_critique_provider: critiqueProvider || null,
        model_critique: critiqueModel || null,
        model_compact_provider: compactProvider || null,
        model_compact: compactModel || null,
        temperature,
        max_tokens: maxTokens,
      });

      // Dispatch custom event to notify other components
      window.dispatchEvent(new CustomEvent('config-updated', {
        detail: {
          model_provider: normalProvider,
          model: normalModel,
          temperature,
          max_tokens: maxTokens,
        }
      }));

      // Show success feedback
      setSaveSuccess(true);

      // Hide success message after 3 seconds
      setTimeout(() => {
        setSaveSuccess(false);
      }, 3000);
    } catch (error) {
      console.error('Failed to save settings:', error);
      alert('Failed to save settings');
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="flex items-center gap-2 text-gray-600">
          <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin" />
          <span>Loading settings...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Success Message */}
      {saveSuccess && (
        <div className="flex items-center gap-2 px-4 py-3 bg-green-50 border border-green-200 rounded-lg text-green-800 animate-fade-in">
          <svg className="w-5 h-5 text-green-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
          <span className="text-sm font-medium">Settings saved successfully!</span>
        </div>
      )}

      {/* Header Info */}
      <div className="bg-blue-50 border border-blue-200 rounded-lg p-4">
        <div className="flex items-start gap-3">
          <svg className="w-5 h-5 text-blue-600 flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <div className="flex-1">
            <h4 className="text-sm font-semibold text-blue-900 mb-1">Five Model System</h4>
            <p className="text-xs text-blue-700 leading-relaxed">
              Configure different models for different tasks: <strong>Normal</strong> for standard coding,
              <strong> Thinking</strong> for complex reasoning, <strong>Critique</strong> for self-critique
              of reasoning, <strong>Compact</strong> for context compaction summaries, and <strong>Vision</strong> for
              image processing. Optional models fall back: Critique → Thinking → Normal, Compact → Normal, Vision → disabled.
            </p>
          </div>
        </div>
      </div>

      {/* Normal Model */}
      <ModelSlot
        title="Normal Model"
        description="For standard coding tasks and general-purpose operations"
        icon={
          <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
          </svg>
        }
        providers={providers}
        selectedProvider={normalProvider}
        selectedModel={normalModel}
        onProviderChange={setNormalProvider}
        onModelChange={setNormalModel}
      />

      {/* Thinking Model */}
      <ModelSlot
        title="Thinking Model"
        description="For complex reasoning and planning tasks (falls back to Normal if not set)"
        icon={
          <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
          </svg>
        }
        providers={providers}
        selectedProvider={thinkingProvider}
        selectedModel={thinkingModel}
        onProviderChange={setThinkingProvider}
        onModelChange={setThinkingModel}
        optional
        notSetText="Use Normal Model"
      />

      {/* Critique Model */}
      <ModelSlot
        title="Critique Model"
        description="For self-critique of reasoning outputs (falls back to Thinking, then Normal)"
        icon={
          <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        }
        providers={providers}
        selectedProvider={critiqueProvider}
        selectedModel={critiqueModel}
        onProviderChange={setCritiqueProvider}
        onModelChange={setCritiqueModel}
        optional
        notSetText="Use Thinking Model"
      />

      {/* Compact Model */}
      <ModelSlot
        title="Compact Model"
        description="For context compaction summaries (falls back to Normal)"
        icon={
          <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
          </svg>
        }
        providers={providers}
        selectedProvider={compactProvider}
        selectedModel={compactModel}
        onProviderChange={setCompactProvider}
        onModelChange={setCompactModel}
        optional
        notSetText="Use Normal Model"
      />

      {/* Vision Model */}
      <ModelSlot
        title="Vision Model"
        description="For image processing and multi-modal tasks (vision unavailable if not set)"
        icon={
          <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
          </svg>
        }
        providers={providers}
        selectedProvider={visionProvider}
        selectedModel={visionModel}
        onProviderChange={setVisionProvider}
        onModelChange={setVisionModel}
        optional
        notSetText="Vision Disabled"
      />

      {/* Global Settings */}
      <div className="border-t border-gray-200 pt-6 space-y-4">
        <h3 className="text-sm font-semibold text-gray-900">Global Settings</h3>

        {/* Temperature */}
        <div>
          <label className="block text-sm font-medium text-gray-900 mb-2">
            Temperature: {temperature.toFixed(2)}
          </label>
          <input
            type="range"
            min="0"
            max="2"
            step="0.1"
            value={temperature}
            onChange={(e) => setTemperature(parseFloat(e.target.value))}
            className="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer"
          />
          <div className="flex justify-between text-xs text-gray-500 mt-1">
            <span>Precise</span>
            <span>Balanced</span>
            <span>Creative</span>
          </div>
        </div>

        {/* Max Tokens */}
        <div>
          <label className="block text-sm font-medium text-gray-900 mb-2">
            Max Tokens
          </label>
          <input
            type="number"
            min="100"
            max="32000"
            step="100"
            value={maxTokens}
            onChange={(e) => setMaxTokens(parseInt(e.target.value))}
            className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
          />
          <p className="mt-1 text-xs text-gray-500">
            Maximum number of tokens to generate in the response
          </p>
        </div>
      </div>

      {/* Save Button */}
      <div className="pt-4 border-t border-gray-200 space-y-3">
        <button
          onClick={handleSave}
          disabled={saving}
          className="w-full px-4 py-3 bg-gradient-to-r from-blue-500 to-purple-600 hover:from-blue-600 hover:to-purple-700 text-white rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-all font-medium shadow-md hover:shadow-lg"
        >
          {saving ? (
            <span className="flex items-center justify-center gap-2">
              <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
              Saving...
            </span>
          ) : (
            'Save Changes'
          )}
        </button>

        {/* Success Message */}
        {saveSuccess && (
          <div className="flex items-center gap-2 px-4 py-3 bg-green-50 border border-green-200 rounded-lg text-green-800 animate-fade-in">
            <svg className="w-5 h-5 text-green-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
            <span className="text-sm font-medium">Settings saved successfully!</span>
          </div>
        )}
      </div>
    </div>
  );
}
