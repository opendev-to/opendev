import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { apiClient } from '../../api/client';
import { ModelSlot } from '../Settings/ModelSlot';
import type { Provider } from '../Settings/ModelSlot';
import { useToastStore } from '../../stores/toast';

interface SessionModelModalProps {
  sessionId: string | null;
  sessionLabel: string;
  onClose: () => void;
}

export function SessionModelModal({ sessionId, sessionLabel, onClose }: SessionModelModalProps) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [hasExistingOverlay, setHasExistingOverlay] = useState(false);
  const addToast = useToastStore(state => state.addToast);

  // Model slots
  const [normalProvider, setNormalProvider] = useState('');
  const [normalModel, setNormalModel] = useState('');
  const [thinkingProvider, setThinkingProvider] = useState('');
  const [thinkingModel, setThinkingModel] = useState('');
  const [critiqueProvider, setCritiqueProvider] = useState('');
  const [critiqueModel, setCritiqueModel] = useState('');
  const [compactProvider, setCompactProvider] = useState('');
  const [compactModel, setCompactModel] = useState('');
  const [visionProvider, setVisionProvider] = useState('');
  const [visionModel, setVisionModel] = useState('');

  useEffect(() => {
    if (sessionId) {
      loadData();
    }
  }, [sessionId]);

  const loadData = async () => {
    if (!sessionId) return;
    try {
      setLoading(true);
      const [providersData, configData, overlayData] = await Promise.all([
        apiClient.listProviders(),
        apiClient.getConfig(),
        apiClient.getSessionModel(sessionId),
      ]);

      setProviders(providersData);

      const hasOverlay = Object.keys(overlayData).length > 0;
      setHasExistingOverlay(hasOverlay);

      // Use overlay values if set, otherwise fall back to global config
      setNormalProvider(overlayData.model_provider || configData.model_provider || '');
      setNormalModel(overlayData.model || configData.model || '');
      setThinkingProvider(overlayData.model_thinking_provider || configData.model_thinking_provider || '');
      setThinkingModel(overlayData.model_thinking || configData.model_thinking || '');
      setCritiqueProvider(overlayData.model_critique_provider || configData.model_critique_provider || '');
      setCritiqueModel(overlayData.model_critique || configData.model_critique || '');
      setCompactProvider(overlayData.model_compact_provider || configData.model_compact_provider || '');
      setCompactModel(overlayData.model_compact || configData.model_compact || '');
      setVisionProvider(overlayData.model_vlm_provider || configData.model_vlm_provider || '');
      setVisionModel(overlayData.model_vlm || configData.model_vlm || '');
    } catch (error) {
      console.error('Failed to load session model data:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!sessionId) return;
    try {
      setSaving(true);

      await apiClient.updateSessionModel(sessionId, {
        model_provider: normalProvider || null,
        model: normalModel || null,
        model_thinking_provider: thinkingProvider || null,
        model_thinking: thinkingModel || null,
        model_critique_provider: critiqueProvider || null,
        model_critique: critiqueModel || null,
        model_compact_provider: compactProvider || null,
        model_compact: compactModel || null,
        model_vlm_provider: visionProvider || null,
        model_vlm: visionModel || null,
      });

      setHasExistingOverlay(true);
      addToast('Session model updated', 'success');
      onClose();
    } catch (error) {
      console.error('Failed to save session model:', error);
      addToast('Failed to save session model', 'error');
    } finally {
      setSaving(false);
    }
  };

  const handleClear = async () => {
    if (!sessionId) return;
    try {
      setSaving(true);
      await apiClient.clearSessionModel(sessionId);
      setHasExistingOverlay(false);

      // Reload with global defaults
      const configData = await apiClient.getConfig();
      setNormalProvider(configData.model_provider || '');
      setNormalModel(configData.model || '');
      setThinkingProvider(configData.model_thinking_provider || '');
      setThinkingModel(configData.model_thinking || '');
      setCritiqueProvider(configData.model_critique_provider || '');
      setCritiqueModel(configData.model_critique || '');
      setCompactProvider(configData.model_compact_provider || '');
      setCompactModel(configData.model_compact || '');
      setVisionProvider(configData.model_vlm_provider || '');
      setVisionModel(configData.model_vlm || '');

      addToast('Session model cleared', 'success');
      onClose();
    } catch (error) {
      console.error('Failed to clear session model:', error);
      addToast('Failed to clear session model', 'error');
    } finally {
      setSaving(false);
    }
  };

  if (!sessionId) return null;

  const modalContent = (
    <div
      className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-2xl shadow-2xl w-full max-w-lg max-h-[90vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-200 flex-shrink-0">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-lg font-bold text-gray-900">Session Models</h2>
              <p className="text-xs text-gray-500 mt-0.5">{sessionLabel}</p>
            </div>
            <button
              onClick={onClose}
              className="w-8 h-8 rounded-lg flex items-center justify-center hover:bg-gray-100 text-gray-400 hover:text-gray-600"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Info banner */}
          <div className="mt-3 px-3 py-2 bg-amber-50 border border-amber-200 rounded-lg">
            <p className="text-xs text-amber-800">
              Override models for this session only. Changes don't affect global settings.
            </p>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <div className="flex items-center gap-2 text-gray-600">
                <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin" />
                <span className="text-sm">Loading...</span>
              </div>
            </div>
          ) : (
            <>
              <ModelSlot
                title="Normal Model"
                description="Standard coding tasks"
                icon={<svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" /></svg>}
                providers={providers}
                selectedProvider={normalProvider}
                selectedModel={normalModel}
                onProviderChange={setNormalProvider}
                onModelChange={setNormalModel}
              />

              <ModelSlot
                title="Thinking Model"
                description="Complex reasoning (falls back to Normal)"
                icon={<svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" /></svg>}
                providers={providers}
                selectedProvider={thinkingProvider}
                selectedModel={thinkingModel}
                onProviderChange={setThinkingProvider}
                onModelChange={setThinkingModel}
                optional
                notSetText="Use Normal Model"
              />

              <ModelSlot
                title="Critique Model"
                description="Self-critique (falls back to Thinking)"
                icon={<svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>}
                providers={providers}
                selectedProvider={critiqueProvider}
                selectedModel={critiqueModel}
                onProviderChange={setCritiqueProvider}
                onModelChange={setCritiqueModel}
                optional
                notSetText="Use Thinking Model"
              />

              <ModelSlot
                title="Compact Model"
                description="Context compaction (falls back to Normal)"
                icon={<svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" /></svg>}
                providers={providers}
                selectedProvider={compactProvider}
                selectedModel={compactModel}
                onProviderChange={setCompactProvider}
                onModelChange={setCompactModel}
                optional
                notSetText="Use Normal Model"
              />

              <ModelSlot
                title="Vision Model"
                description="Image processing (disabled if not set)"
                icon={<svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>}
                providers={providers}
                selectedProvider={visionProvider}
                selectedModel={visionModel}
                onProviderChange={setVisionProvider}
                onModelChange={setVisionModel}
                optional
                notSetText="Vision Disabled"
              />
            </>
          )}
        </div>

        {/* Footer */}
        {!loading && (
          <div className="px-6 py-4 border-t border-gray-200 flex-shrink-0 space-y-3">
            <div className="flex gap-3">
              {hasExistingOverlay && (
                <button
                  onClick={handleClear}
                  disabled={saving}
                  className="px-4 py-2.5 border border-red-300 text-red-700 rounded-lg hover:bg-red-50 disabled:opacity-50 disabled:cursor-not-allowed font-medium text-sm"
                >
                  Clear Overrides
                </button>
              )}
              <button
                onClick={handleSave}
                disabled={saving}
                className="flex-1 px-4 py-2.5 bg-gradient-to-r from-amber-500 to-orange-600 hover:from-amber-600 hover:to-orange-700 text-white rounded-lg disabled:opacity-50 disabled:cursor-not-allowed font-medium text-sm shadow-md hover:shadow-lg"
              >
                {saving ? (
                  <span className="flex items-center justify-center gap-2">
                    <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                    Saving...
                  </span>
                ) : (
                  'Save'
                )}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );

  return createPortal(modalContent, document.body);
}
