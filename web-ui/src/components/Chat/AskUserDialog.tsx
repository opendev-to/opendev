import { useState, useEffect } from 'react';
import { useChatStore } from '../../stores/chat';
import type { AskUserQuestion } from '../../types';

export function AskUserDialog() {
  const pendingAskUser = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.pendingAskUser ?? null : null;
  });
  const respondToAskUser = useChatStore(state => state.respondToAskUser);

  const [currentIdx, setCurrentIdx] = useState(0);
  const [answers, setAnswers] = useState<Record<string, any>>({});
  const [selectedOptions, setSelectedOptions] = useState<Set<number>>(new Set());
  const [otherText, setOtherText] = useState('');
  const [showOther, setShowOther] = useState(false);

  // Reset state when new request comes in
  useEffect(() => {
    if (pendingAskUser) {
      setCurrentIdx(0);
      setAnswers({});
      setSelectedOptions(new Set());
      setOtherText('');
      setShowOther(false);
    }
  }, [pendingAskUser]);

  // Keyboard shortcuts
  useEffect(() => {
    if (!pendingAskUser) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      const questions = pendingAskUser.questions;
      if (!questions || currentIdx >= questions.length) return;
      const q = questions[currentIdx];

      // Number keys to select options
      const num = parseInt(e.key);
      if (!isNaN(num) && num >= 1 && num <= q.options.length) {
        e.preventDefault();
        const optIdx = num - 1;
        if (q.multi_select) {
          setSelectedOptions(prev => {
            const next = new Set(prev);
            if (next.has(optIdx)) next.delete(optIdx);
            else next.add(optIdx);
            return next;
          });
        } else {
          setSelectedOptions(new Set([optIdx]));
          setShowOther(false);
        }
      }

      // 'o' for Other
      if (e.key === 'o' || e.key === 'O') {
        if (!q.multi_select) {
          setSelectedOptions(new Set());
        }
        setShowOther(true);
      }

      // Enter to confirm/next
      if (e.key === 'Enter' && !e.shiftKey) {
        // Don't intercept if typing in the Other input
        if (showOther && document.activeElement?.tagName === 'INPUT') {
          e.preventDefault();
          handleNext();
          return;
        }
        if (selectedOptions.size > 0 || showOther) {
          e.preventDefault();
          handleNext();
        }
      }

      // Escape to cancel
      if (e.key === 'Escape') {
        e.preventDefault();
        handleCancel();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pendingAskUser, currentIdx, selectedOptions, showOther, otherText]);

  if (!pendingAskUser) return null;

  const questions = pendingAskUser.questions;
  if (!questions || questions.length === 0) return null;

  const currentQuestion: AskUserQuestion = questions[currentIdx];
  const isLastQuestion = currentIdx === questions.length - 1;

  const handleOptionClick = (optIdx: number) => {
    if (currentQuestion.multi_select) {
      setSelectedOptions(prev => {
        const next = new Set(prev);
        if (next.has(optIdx)) next.delete(optIdx);
        else next.add(optIdx);
        return next;
      });
      setShowOther(false);
    } else {
      setSelectedOptions(new Set([optIdx]));
      setShowOther(false);
    }
  };

  const handleOtherClick = () => {
    if (!currentQuestion.multi_select) {
      setSelectedOptions(new Set());
    }
    setShowOther(true);
  };

  const handleNext = () => {
    // Save current answer
    let answer: any;
    if (showOther && otherText.trim()) {
      answer = currentQuestion.multi_select
        ? [...Array.from(selectedOptions).map(i => currentQuestion.options[i].label), otherText.trim()]
        : otherText.trim();
    } else if (selectedOptions.size > 0) {
      if (currentQuestion.multi_select) {
        answer = Array.from(selectedOptions).map(i => currentQuestion.options[i].label);
      } else {
        const idx = Array.from(selectedOptions)[0];
        answer = currentQuestion.options[idx].label;
      }
    } else {
      return; // Nothing selected
    }

    const newAnswers = { ...answers, [String(currentIdx)]: answer };
    setAnswers(newAnswers);

    if (isLastQuestion) {
      // Submit
      respondToAskUser(pendingAskUser.request_id, newAnswers);
    } else {
      // Next question
      setCurrentIdx(currentIdx + 1);
      setSelectedOptions(new Set());
      setOtherText('');
      setShowOther(false);
    }
  };

  const handleBack = () => {
    if (currentIdx > 0) {
      setCurrentIdx(currentIdx - 1);
      setSelectedOptions(new Set());
      setOtherText('');
      setShowOther(false);
    }
  };

  const handleCancel = () => {
    respondToAskUser(pendingAskUser.request_id, null);
  };

  return (
    <div className="fixed inset-0 bg-black/20 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in">
      <div className="bg-bg-000 rounded-xl shadow-2xl border border-border-300/15 max-w-lg w-full mx-4 animate-slide-up">
        {/* Header */}
        <div className="border-b border-border-300/15 px-6 py-4">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold text-text-000">Question</h2>
            {questions.length > 1 && (
              <div className="flex items-center gap-1.5">
                {questions.map((_, i) => (
                  <div
                    key={i}
                    className={`w-2 h-2 rounded-full transition-colors ${
                      i === currentIdx ? 'bg-accent-secondary-100' : i < currentIdx ? 'bg-accent-secondary-100/50' : 'bg-bg-400'
                    }`}
                  />
                ))}
              </div>
            )}
          </div>
          {currentQuestion.header && (
            <span className="inline-block mt-2 px-2 py-0.5 bg-accent-secondary-900 text-accent-secondary-100 text-xs font-medium rounded">
              {currentQuestion.header}
            </span>
          )}
        </div>

        {/* Question */}
        <div className="px-6 py-5 space-y-4">
          <p className="text-sm text-text-100 font-medium leading-relaxed">
            {currentQuestion.question}
          </p>

          {/* Options */}
          <div className="space-y-2">
            {currentQuestion.options.map((opt, i) => {
              const isSelected = selectedOptions.has(i);
              return (
                <button
                  key={i}
                  onClick={() => handleOptionClick(i)}
                  className={`w-full px-4 py-3 text-sm text-left rounded-lg border-2 transition-all flex items-center gap-3 group ${
                    isSelected
                      ? 'border-accent-secondary-100/50 bg-accent-secondary-900/50'
                      : 'border-border-300/15 hover:border-accent-secondary-100/30 hover:bg-bg-100'
                  }`}
                >
                  {/* Selection indicator */}
                  <div className={`w-5 h-5 rounded${currentQuestion.multi_select ? '' : '-full'} border-2 flex items-center justify-center flex-shrink-0 ${
                    isSelected ? 'border-accent-secondary-100 bg-accent-secondary-100' : 'border-border-300/30'
                  }`}>
                    {isSelected && (
                      <svg className="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
                      </svg>
                    )}
                  </div>

                  <div className="flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-mono text-text-500 bg-bg-200 px-1.5 py-0.5 rounded">
                        {i + 1}
                      </span>
                      <span className={`font-medium ${isSelected ? 'text-text-000' : 'text-text-100'}`}>
                        {opt.label}
                      </span>
                    </div>
                    {opt.description && (
                      <p className="text-xs text-text-400 mt-1 ml-7">{opt.description}</p>
                    )}
                  </div>
                </button>
              );
            })}

            {/* Other option */}
            <button
              onClick={handleOtherClick}
              className={`w-full px-4 py-3 text-sm text-left rounded-lg border-2 border-dashed transition-all ${
                showOther
                  ? 'border-accent-secondary-100/50 bg-accent-secondary-900/50'
                  : 'border-border-300/20 hover:border-accent-secondary-100/30 hover:bg-bg-100'
              }`}
            >
              <span className={`font-medium ${showOther ? 'text-text-000' : 'text-text-300'}`}>
                Other...
              </span>
            </button>

            {/* Other text input */}
            {showOther && (
              <input
                type="text"
                value={otherText}
                onChange={e => setOtherText(e.target.value)}
                placeholder="Type your answer..."
                className="w-full px-4 py-2.5 border border-border-300/20 rounded-lg text-sm text-text-000 bg-bg-000 focus:outline-none focus:ring-2 focus:ring-accent-secondary-100 focus:border-accent-secondary-100 placeholder-text-500"
                autoFocus
              />
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="border-t border-border-300/15 px-6 py-4 bg-bg-100 flex items-center justify-between">
          <button
            onClick={handleCancel}
            className="px-3 py-1.5 text-sm text-text-400 hover:text-text-200 transition-colors"
          >
            Cancel
          </button>

          <div className="flex items-center gap-2">
            {currentIdx > 0 && (
              <button
                onClick={handleBack}
                className="px-4 py-2 text-sm font-medium text-text-200 bg-bg-000 border border-border-300/20 rounded-lg hover:bg-bg-200 transition-colors"
              >
                Back
              </button>
            )}
            <button
              onClick={handleNext}
              disabled={selectedOptions.size === 0 && !(showOther && otherText.trim())}
              className="px-4 py-2 text-sm font-medium text-white bg-accent-secondary-100 rounded-lg hover:bg-accent-secondary-100/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {isLastQuestion ? 'Submit' : 'Next'}
            </button>
          </div>
        </div>

        {/* Keyboard hints */}
        <div className="text-center pb-3">
          <p className="text-xs text-text-500">
            Press <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">1</kbd>-<kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">{currentQuestion.options.length}</kbd> to select, <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">Enter</kbd> to confirm, <kbd className="px-1 py-0.5 bg-bg-200 border border-border-300/20 rounded text-xs font-mono">Esc</kbd> to cancel
          </p>
        </div>
      </div>
    </div>
  );
}
