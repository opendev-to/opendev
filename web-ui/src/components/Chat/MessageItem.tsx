import React from 'react';
import ReactMarkdown from 'react-markdown';
import { ToolCallMessage } from './ToolCallMessage';
import { ThinkingBlock } from './ThinkingBlock';

interface MessageItemProps {
  message: any; // Using any for now to simplify, ideally should import ChatMessage from types
  index: number;
  isNewMessage: boolean;
  prevMessageCount: number;
  thinkingLevel: string;
  isLoading: boolean;
  isLastMessage: boolean;
}

const markdownComponents = {
  pre({ children }: any) {
    return (
      <pre className="rounded-lg p-3 overflow-x-auto my-2 bg-bg-300 border border-border-300/15">
        {children}
      </pre>
    );
  },
  code({ className, children, ...props }: any) {
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
  p({ children }: any) {
    return <p className="mb-2 last:mb-0 text-text-200 text-sm">{children}</p>;
  },
  ul({ children }: any) {
    return <ul className="list-disc pl-5 space-y-1 mb-2 text-text-200 text-sm">{children}</ul>;
  },
  ol({ children }: any) {
    return <ol className="list-decimal pl-5 space-y-1 mb-2 text-text-200 text-sm">{children}</ol>;
  },
  li({ children }: any) {
    return <li className="text-text-200 text-sm">{children}</li>;
  },
  strong({ children }: any) {
    return <strong className="font-semibold text-text-000 text-sm">{children}</strong>;
  },
  a({ children, href }: any) {
    return <a href={href} className="link-underline text-accent-secondary-100 hover:text-accent-secondary-100/80 text-sm" target="_blank" rel="noopener noreferrer">{children}</a>;
  },
};

export const MessageItem = React.memo(function MessageItem({
  message,
  index,
  isNewMessage,
  prevMessageCount,
  thinkingLevel,
  isLoading,
  isLastMessage
}: MessageItemProps) {
  // Nested tool calls get indentation
  const depthMargin = message.depth ? `ml-${Math.min(message.depth * 6, 24)}` : '';

  // Stagger animation for new messages
  const staggerStyle = isNewMessage
    ? { animationDelay: `${(index - prevMessageCount) * 50}ms`, animationFillMode: 'both' as const }
    : undefined;

  // Render tool calls with special component
  if (message.role === 'tool_call') {
    const hasResult = message.tool_result != null && Object.keys(message.tool_result).length > 0;
    return (
      <div className={`animate-slide-up ${depthMargin}`} style={{ ...(message.depth ? { marginLeft: `${message.depth * 1.5}rem` } : {}), ...staggerStyle }}>
        <ToolCallMessage message={message} hasResult={hasResult} />
      </div>
    );
  }

  // Render thinking blocks (only when thinking level is not Off)
  if (message.role === 'thinking') {
    if (thinkingLevel === 'Off') return null;
    const isLastThinking = isLoading && isLastMessage;
    return <ThinkingBlock content={message.content} level={message.metadata?.level} isActive={isLastThinking} />;
  }

  const isUser = message.role === 'user';

  return (
    <div className="animate-slide-up" style={staggerStyle}>
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
                components={markdownComponents}
              >
                {message.content}
              </ReactMarkdown>
            </div>
          </div>
        </div>
      )}
    </div>
  );
});
