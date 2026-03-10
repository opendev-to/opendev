import { useEffect, useRef, useState, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import { useChatStore } from '../../stores/chat';
import { ToolCallMessage } from './ToolCallMessage';
import { ThinkingBlock } from './ThinkingBlock';
import { SPINNER_FRAMES, THINKING_VERBS, SPINNER_COLORS } from '../../constants/spinner';

export function MessageList() {
  const messages = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.messages ?? [] : [];
  });
  const isLoading = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.isLoading ?? false : false;
  });
  const progressMessage = useChatStore(state => {
    const sid = state.currentSessionId;
    return sid ? state.sessionStates[sid]?.progressMessage ?? null : null;
  });
  const thinkingLevel = useChatStore(state => state.thinkingLevel);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll state
  const [userHasScrolled, setUserHasScrolled] = useState(false);
  const isNearBottomRef = useRef(true);

  // Spinner animation state
  const [spinnerIndex, setSpinnerIndex] = useState(0);
  const [verbIndex, setVerbIndex] = useState(0);
  const [colorIndex, setColorIndex] = useState(0);

  // Braille halo animation for welcome screen
  const [brailleOffset, setBrailleOffset] = useState(0);

  // Stagger animation: track previous message count
  const prevMessageCountRef = useRef(messages.length);

  // Smart auto-scroll: track user scroll position
  const handleScroll = useCallback(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
    const nearBottom = distanceFromBottom < 50;

    isNearBottomRef.current = nearBottom;

    if (nearBottom) {
      setUserHasScrolled(false);
    } else {
      setUserHasScrolled(true);
    }
  }, []);

  // Auto-scroll on new messages (only if user hasn't scrolled up)
  useEffect(() => {
    if (!userHasScrolled) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, userHasScrolled, progressMessage]);

  // Animate spinner when loading
  useEffect(() => {
    if (!isLoading) return;

    const spinnerInterval = setInterval(() => {
      setSpinnerIndex(prev => (prev + 1) % SPINNER_FRAMES.length);
      setColorIndex(prev => (prev + 1) % SPINNER_COLORS.length);
    }, 100); // Match terminal speed: 100ms

    const verbInterval = setInterval(() => {
      setVerbIndex(prev => (prev + 1) % THINKING_VERBS.length);
    }, 2000); // Change verb every 2 seconds

    return () => {
      clearInterval(spinnerInterval);
      clearInterval(verbInterval);
    };
  }, [isLoading]);

  // Animate braille halo when welcome screen is visible
  useEffect(() => {
    if (messages.length > 0) return;
    const interval = setInterval(() => {
      setBrailleOffset(prev => (prev + 1) % SPINNER_FRAMES.length);
    }, 100);
    return () => clearInterval(interval);
  }, [messages.length]);

  // Update stagger ref after new messages settle
  useEffect(() => {
    const timer = setTimeout(() => {
      prevMessageCountRef.current = messages.length;
    }, 500);
    return () => clearTimeout(timer);
  }, [messages.length]);

  // Custom Page Up/Page Down handling with shorter scroll distance
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!scrollContainerRef.current) return;

      const scrollDistance = 300; // Shorter scroll distance (default is ~viewport height)

      if (e.key === 'PageUp') {
        e.preventDefault();
        scrollContainerRef.current.scrollBy({
          top: -scrollDistance,
          behavior: 'smooth'
        });
      } else if (e.key === 'PageDown') {
        e.preventDefault();
        scrollContainerRef.current.scrollBy({
          top: scrollDistance,
          behavior: 'smooth'
        });
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  if (messages.length === 0) {
    return (
      <div className="relative flex items-center justify-center h-full px-6 bg-bg-100 overflow-hidden">
        {/* Background watermark layer */}
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
          {/* "OpenDev" breathing text */}
          <span className="text-5xl md:text-7xl font-mono font-bold tracking-wider text-bg-300 animate-breathe select-none">
            OpenDev
          </span>
          {/* Orbiting braille halo ring */}
          <div className="absolute animate-spin-slow" style={{ width: 360, height: 360 }}>
            {Array.from({ length: 24 }).map((_, i) => {
              const angle = (i / 24) * 360;
              const char = SPINNER_FRAMES[(i + brailleOffset) % SPINNER_FRAMES.length];
              return (
                <span
                  key={i}
                  className="absolute text-lg font-mono text-bg-300"
                  style={{
                    left: '50%',
                    top: '50%',
                    transform: `rotate(${angle}deg) translateX(180px) rotate(-${angle}deg)`,
                  }}
                >
                  {char}
                </span>
              );
            })}
          </div>
        </div>
        {/* Foreground welcome content */}
        <div className="relative z-10 text-center">
          <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-bg-200 flex items-center justify-center">
            <svg className="w-8 h-8 text-text-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
            </svg>
          </div>
          <h2 className="text-xl font-semibold text-text-000 mb-2">Welcome to OpenDev</h2>
          <p className="text-sm text-text-300">Start a conversation with your AI coding assistant</p>
        </div>
      </div>
    );
  }

  return (
    <div ref={scrollContainerRef} className="flex-1 overflow-y-auto bg-bg-100" onScroll={handleScroll}>
      <div className="max-w-5xl mx-auto py-6 px-4 md:px-8 space-y-4">
        {messages.map((message, index) => {
          // Nested tool calls get indentation
          const depthMargin = message.depth ? `ml-${Math.min(message.depth * 6, 24)}` : '';

          // Stagger animation for new messages
          const isNewMessage = index >= prevMessageCountRef.current;
          const staggerStyle = isNewMessage
            ? { animationDelay: `${(index - prevMessageCountRef.current) * 50}ms`, animationFillMode: 'both' as const }
            : undefined;

          // Render tool calls with special component
          if (message.role === 'tool_call') {
            const hasResult = message.tool_result != null && Object.keys(message.tool_result).length > 0;
            return (
              <div key={index} className={`animate-slide-up ${depthMargin}`} style={{ ...(message.depth ? { marginLeft: `${message.depth * 1.5}rem` } : {}), ...staggerStyle }}>
                <ToolCallMessage message={message} hasResult={hasResult} />
              </div>
            );
          }

          // Render thinking blocks (only when thinking level is not Off)
          if (message.role === 'thinking') {
            if (thinkingLevel === 'Off') return null;
            const isLastThinking = isLoading && index === messages.length - 1;
            return <ThinkingBlock key={index} content={message.content} level={message.metadata?.level} isActive={isLastThinking} />;
          }

          const isUser = message.role === 'user';

          return (
            <div key={index} className="animate-slide-up" style={staggerStyle}>
              {isUser ? (
                <div className="bg-bg-200 border border-border-300/15 rounded-lg px-4 py-3">
                  <div className="flex items-start gap-3">
                    <span className="text-accent-main-100 font-mono text-sm font-bold flex-shrink-0">#</span>
                    <div className="flex-1 text-text-000 font-mono text-sm">
                      {message.content}
                    </div>
                  </div>
                </div>
              ) : (
                <div className="bg-bg-000 border border-border-300/15 rounded-lg px-4 py-3">
                  <div className="flex items-start gap-3">
                    <span className="text-text-400 font-mono text-sm font-medium flex-shrink-0">&#10095;</span>
                    <div className="flex-1 prose prose-sm max-w-none code-hover">
                      <ReactMarkdown
                        components={{
                          pre({ children }) {
                            return (
                              <pre className="rounded-lg p-3 overflow-x-auto my-2 bg-bg-300 border border-border-300/15">
                                {children}
                              </pre>
                            );
                          },
                          code({ className, children, ...props }) {
                            const language = /language-(\w+)/.exec(className || '')?.[1];
                            if (language) {
                              return <code className="text-text-000 text-sm font-mono" data-language={language} {...props}>{children}</code>;
                            }
                            return (
                              <code className="text-sm px-1.5 py-0.5 rounded font-mono bg-bg-200 text-text-100 border border-border-300/20" {...props}>
                                {children}
                              </code>
                            );
                          },
                          p({ children }) {
                            return <p className="mb-2 last:mb-0 text-text-200 text-sm">{children}</p>;
                          },
                          ul({ children }) {
                            return <ul className="list-disc pl-5 space-y-1 mb-2 text-text-200 text-sm">{children}</ul>;
                          },
                          ol({ children }) {
                            return <ol className="list-decimal pl-5 space-y-1 mb-2 text-text-200 text-sm">{children}</ol>;
                          },
                          li({ children }) {
                            return <li className="text-text-200 text-sm">{children}</li>;
                          },
                          strong({ children }) {
                            return <strong className="font-semibold text-text-000 text-sm">{children}</strong>;
                          },
                          a({ children, href }) {
                            return <a href={href} className="link-underline text-accent-secondary-100 hover:text-accent-secondary-100/80 text-sm" target="_blank" rel="noopener noreferrer">{children}</a>;
                          },
                        }}
                      >
                        {message.content}
                      </ReactMarkdown>
                    </div>
                  </div>
                </div>
              )}
            </div>
          );
        })}

        {/* Progress indicator */}
        {progressMessage && (
          <div className="bg-bg-000 border border-border-300/15 rounded-lg px-4 py-3 animate-fade-in">
            <div className="flex items-center gap-3">
              <span className={`text-base font-medium ${SPINNER_COLORS[colorIndex]} transition-colors duration-100`}>
                {SPINNER_FRAMES[spinnerIndex]}
              </span>
              <span className="text-sm text-text-300 font-medium">
                {progressMessage}
              </span>
            </div>
          </div>
        )}

        {isLoading && !progressMessage && (
          <div className="bg-bg-000 border border-border-300/15 rounded-lg px-4 py-3 animate-fade-in">
            <div className="flex items-center gap-3">
              <span className={`text-base font-medium ${SPINNER_COLORS[colorIndex]} transition-colors duration-100`}>
                {SPINNER_FRAMES[spinnerIndex]}
              </span>
              <span className="text-sm text-text-300 font-medium">
                {THINKING_VERBS[verbIndex]}...
              </span>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>
    </div>
  );
}
