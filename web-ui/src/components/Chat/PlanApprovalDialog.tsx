import { useState, useEffect } from 'react';
import { useChatStore } from '../../stores/chat';
import ReactMarkdown from 'react-markdown';

export function PlanApprovalDialog() {
  const pendingPlanApproval = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.pendingPlanApproval ?? null : null;
  });
  const respondToPlanApproval = useChatStore(state => state.respondToPlanApproval);

  const [selectedIndex, setSelectedIndex] = useState(0);
  const [showFeedback, setShowFeedback] = useState(false);
  const [feedback, setFeedback] = useState('');

  const options = [
    {
      label: 'Start implementation',
      description: 'Auto-approve file edits during implementation.',
      action: 'approve_auto',
    },
    {
      label: 'Start implementation (review edits)',
      description: 'Review each file edit before it\'s applied.',
      action: 'approve',
    },
    {
      label: 'Revise plan',
      description: 'Stay in plan mode and provide feedback.',
      action: 'modify',
    },
  ];

  // Reset state when new request comes in
  useEffect(() => {
    if (pendingPlanApproval) {
      setSelectedIndex(0);
      setShowFeedback(false);
      setFeedback('');
    }
  }, [pendingPlanApproval]);

  // Keyboard shortcuts
  useEffect(() => {
    if (!pendingPlanApproval) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Number keys to select options
      const num = parseInt(e.key);
      if (!isNaN(num) && num >= 1 && num <= options.length) {
        e.preventDefault();
        setSelectedIndex(num - 1);
        setShowFeedback(false);
      }

      // Enter to confirm
      if (e.key === 'Enter' && !e.shiftKey) {
        if (showFeedback && document.activeElement?.tagName === 'TEXTAREA') {
          return; // Let textarea handle Enter
        }
        e.preventDefault();
        handleConfirm();
      }

      // Escape to revise
      if (e.key === 'Escape') {
        e.preventDefault();
        handleRevise();
      }

      // Arrow keys
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex(prev => (prev - 1 + options.length) % options.length);
        setShowFeedback(false);
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex(prev => (prev + 1) % options.length);
        setShowFeedback(false);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pendingPlanApproval, selectedIndex, showFeedback, feedback]);

  if (!pendingPlanApproval) return null;

  const handleConfirm = () => {
    const action = options[selectedIndex].action;
    if (action === 'modify') {
      if (!showFeedback) {
        setShowFeedback(true);
        return;
      }
      respondToPlanApproval(pendingPlanApproval.request_id, action, feedback);
    } else {
      respondToPlanApproval(pendingPlanApproval.request_id, action);
    }
  };

  const handleRevise = () => {
    setSelectedIndex(2); // Revise plan
    setShowFeedback(true);
  };

  return (
    <div className="fixed inset-0 bg-black/20 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in">
      <div className="bg-bg-000 rounded-xl shadow-2xl border border-border-300/15 max-w-2xl w-full mx-4 max-h-[85vh] flex flex-col animate-slide-up">
        {/* Header */}
        <div className="border-b border-border-300/15 px-6 py-4 flex-shrink-0">
          <div className="flex items-center gap-3">
            <div className="w-2 h-2 rounded-full bg-accent-secondary-100 animate-pulse" />
            <h2 className="text-lg font-semibold text-text-000">Plan Ready for Review</h2>
          </div>
        </div>

        {/* Plan content - scrollable */}
        <div className="flex-1 overflow-y-auto px-6 py-4 min-h-0">
          <div className="prose prose-sm prose-invert max-w-none text-text-200">
            <ReactMarkdown>{pendingPlanApproval.plan_content}</ReactMarkdown>
          </div>
        </div>

        {/* Options */}
        <div className="border-t border-border-300/15 px-6 py-4 flex-shrink-0 space-y-2">
          {options.map((opt, i) => {
            const isSelected = i === selectedIndex;
            return (
              <button
                key={i}
                onClick={() => {
                  setSelectedIndex(i);
                  setShowFeedback(false);
                  if (i !== 2) {
                    respondToPlanApproval(pendingPlanApproval.request_id, opt.action);
                  } else {
                    setShowFeedback(true);
                  }
                }}
                className={`w-full px-4 py-3 text-sm text-left rounded-lg border-2 transition-all flex items-center gap-3 ${
                  isSelected
                    ? 'border-accent-secondary-100/50 bg-accent-secondary-900/50'
                    : 'border-border-300/15 hover:border-accent-secondary-100/30 hover:bg-bg-100'
                }`}
              >
                <span className="text-xs font-mono text-text-500 bg-bg-200 px-1.5 py-0.5 rounded">
                  {i + 1}
                </span>
                <div className="flex-1">
                  <span className={`font-medium ${isSelected ? 'text-text-000' : 'text-text-100'}`}>
                    {opt.label}
                  </span>
                  <span className="text-text-400 text-xs ml-2">{opt.description}</span>
                </div>
              </button>
            );
          })}

          {/* Feedback textarea for revise */}
          {showFeedback && (
            <div className="mt-3 space-y-2">
              <textarea
                value={feedback}
                onChange={e => setFeedback(e.target.value)}
                placeholder="What changes would you like? (optional)"
                className="w-full px-4 py-2.5 border border-border-300/20 rounded-lg text-sm text-text-000 bg-bg-000 focus:outline-none focus:ring-2 focus:ring-accent-secondary-100 focus:border-accent-secondary-100 placeholder-text-500 resize-none"
                rows={3}
                autoFocus
              />
              <button
                onClick={() => respondToPlanApproval(pendingPlanApproval.request_id, 'modify', feedback)}
                className="px-4 py-2 text-sm font-medium text-white bg-accent-secondary-100 rounded-lg hover:bg-accent-secondary-100/90 transition-colors"
              >
                Submit Feedback
              </button>
            </div>
          )}
        </div>

        {/* Keyboard hints */}
        <div className="text-center pb-3 flex-shrink-0">
          <p className="text-xs text-text-500">
            Press{' '}
            <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">1</kbd>-
            <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">3</kbd>{' '}
            to select,{' '}
            <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">Enter</kbd>{' '}
            to confirm,{' '}
            <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">Esc</kbd>{' '}
            to revise
          </p>
        </div>
      </div>
    </div>
  );
}
