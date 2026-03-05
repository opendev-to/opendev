import { useState } from 'react';
import {
  DocumentTextIcon,
  ChatBubbleLeftRightIcon,
  MagnifyingGlassIcon,
  SparklesIcon,
  CodeBracketIcon,
  FolderIcon
} from '@heroicons/react/24/outline';
import './CodeWiki.css';

interface WikiPage {
  id: string;
  title: string;
  type: 'architecture' | 'api' | 'guide' | 'reference' | 'overview';
  path?: string;
  content: string;
  description?: string;
  lastModified: string;
  importance?: number;
  parent?: string;
  children?: WikiPage[];
  relatedFiles?: string[];
  relatedPages?: string[];
  tags?: string[];
  diagrams?: string[];
}

interface DocumentationItem {
  id: string;
  title: string;
  type: 'wiki-page' | 'readme' | 'doc' | 'code' | 'folder';
  path?: string;
  content?: string;
  children?: DocumentationItem[];
  lastModified: string;
  wikiPage?: WikiPage;
}

interface DocumentationViewerProps {
  selectedRepo: string | null;
  searchQuery: string;
  onIndexingChange: (isIndexing: boolean) => void;
}

// Mock wiki pages that demonstrate DeepWiki-style intelligent documentation
const mockWikiPages: WikiPage[] = [
  {
    id: 'overview',
    title: 'OpenDev Architecture Overview',
    type: 'overview',
    description: 'High-level architecture and design principles of OpenDev',
    lastModified: '2 hours ago',
    importance: 10,
    content: `# OpenDev Architecture Overview

OpenDev (Software Engineering CLI) is an AI-powered command-line tool designed to enhance developer productivity through intelligent code assistance and automation.

## Core Components

### 1. Agent System
- **Multi-Agent Architecture**: Specialized agents for different tasks (coding, debugging, testing)
- **Agent Orchestrator**: Coordinates multiple agents for complex workflows
- **Tool Integration**: Extensible tool system for file operations, web access, and more

### 2. Web Interface
- **Real-time Communication**: WebSocket-based live updates
- **Session Management**: Persistent conversation history and workspace management
- **Approval System**: Interactive tool execution approval with detailed previews

### 3. Backend Services
- **Tool Registry**: Centralized tool discovery and execution
- **Configuration Management**: Dynamic configuration with workspace-specific settings
- **WebSocket Server**: Real-time bidirectional communication

## Key Design Principles

- **Modularity**: Each component is independently testable and replaceable
- **Extensibility**: Plugin-based architecture for custom tools and agents
- **Security**: Sandboxed execution environment with approval workflows
- **Performance**: Async execution with intelligent caching and optimization`,
    relatedFiles: ['swecli/core/agent.py', 'swecli/web/server.py', 'swecli/tools/registry.py'],
    relatedPages: ['agent-system', 'web-interface', 'tool-system'],
    tags: ['architecture', 'overview', 'design'],
    diagrams: ['architecture-diagram.svg', 'component-interaction.svg']
  },
  {
    id: 'agent-system',
    title: 'Agent System Architecture',
    type: 'architecture',
    description: 'Detailed breakdown of the multi-agent execution system',
    lastModified: '3 hours ago',
    importance: 9,
    parent: 'overview',
    content: `# Agent System Architecture

The agent system is the core intelligence layer of OpenDev, responsible for understanding user intent and executing complex coding tasks through coordinated tool usage.

## Agent Types

### 1. Normal Agent
- **Purpose**: General coding assistance and problem-solving
- **Capabilities**: Code analysis, file operations, debugging, testing
- **Tool Access**: Full tool registry with approval workflows
- **Primary Use Case**: Daily development tasks and code improvement

### 2. Debug Agent
- **Purpose**: Specialized debugging and error resolution
- **Capabilities**: Stack trace analysis, root cause identification, fix suggestions
- **Tool Access**: Read-only operations with limited write capabilities
- **Primary Use Case**: Troubleshooting and bug fixing

### 3. Planning Agent
- **Purpose**: Complex task decomposition and workflow planning
- **Capabilities**: Task breakdown, dependency analysis, execution planning
- **Tool Access**: Analysis tools without execution capabilities
- **Primary Use Case**: Large-scale refactoring and feature implementation

## Agent Lifecycle

1. **Intent Analysis**: Understanding user requirements and context
2. **Tool Selection**: Choosing appropriate tools for the task
3. **Execution Planning**: Creating step-by-step execution plan
4. **Tool Execution**: Running tools with user approval when required
5. **Result Processing**: Analyzing outputs and determining next steps
6. **Response Generation**: Providing clear, actionable responses`,
    relatedFiles: ['swecli/core/agent.py', 'swecli/agents/normal.py', 'swecli/agents/debug.py'],
    relatedPages: ['tool-system', 'approval-workflow'],
    tags: ['agents', 'architecture', 'execution']
  },
  {
    id: 'web-interface',
    title: 'Web Interface and Real-time Communication',
    type: 'architecture',
    description: 'Modern web-based interface with WebSocket communication',
    lastModified: '1 day ago',
    importance: 8,
    parent: 'overview',
    content: `# Web Interface Architecture

The web interface provides a modern, responsive UI for OpenDev with real-time communication capabilities and session management.

## Frontend Components

### 1. Chat Interface
- **Real-time Messaging**: Live updates via WebSocket connections
- **Tool Call Visualization**: Interactive tool execution with approval dialogs
- **Session Management**: Persistent conversation history and workspace switching
- **Responsive Design**: Optimized for desktop and mobile usage

### 2. Approval System
- **Tool Previews**: Detailed preview of pending operations
- **Interactive Approval**: Approve, deny, or modify tool execution
- **Batch Operations**: Apply approval decisions to multiple similar operations
- **Security Controls**: Configurable approval rules and restrictions

### 3. Workspace Management
- **Session Persistence**: Save and restore conversation contexts
- **File Change Tracking**: Monitor and review all file modifications
- **Configuration**: Workspace-specific settings and preferences

## Backend Services

### WebSocket Server
- **Bidirectional Communication**: Real-time message exchange
- **Connection Management**: Handle multiple concurrent connections
- **Message Routing**: Efficient message distribution and processing
- **Error Handling**: Robust error recovery and reporting

### Session Manager
- **State Management**: Persistent session state and history
- **Workspace Isolation**: Separate contexts for different projects
- **File Change Tracking**: Comprehensive audit trail of modifications
- **Configuration Management**: Dynamic settings and preferences`,
    relatedFiles: ['swecli/web/server.py', 'swecli/web/websocket.py', 'swecli/web/state.py'],
    relatedPages: ['api-reference', 'session-management'],
    tags: ['web', 'websocket', 'ui', 'real-time']
  },
  {
    id: 'api-reference',
    title: 'API Reference',
    type: 'reference',
    description: 'Complete API documentation for OpenDev components',
    lastModified: '6 hours ago',
    importance: 7,
    content: `# API Reference

Complete API documentation for OpenDev components, including REST endpoints, WebSocket messages, and internal APIs.

## REST API Endpoints

### Session Management
- \`POST /api/sessions/create\` - Create new session
- \`GET /api/sessions\` - List all sessions
- \`POST /api/sessions/{id}/resume\` - Resume existing session
- \`DELETE /api/sessions/{id}\` - Delete session

### Chat and Messages
- \`GET /api/chat/messages\` - Get conversation history
- \`POST /api/chat/clear\` - Clear conversation history

### Configuration
- \`GET /api/config\` - Get current configuration
- \`PUT /api/config\` - Update configuration settings
- \`GET /api/config/providers\` - List AI providers

## WebSocket API

### Message Types
- \`query\` - Send user query to agent
- \`approve\` - Respond to tool execution approval
- \`ping\` - Connection health check

### Event Types
- \`user_message\` - New user message
- \`message_start\` - Agent response starting
- \`message_chunk\` - Partial response content
- \`message_complete\` - Full response received
- \`tool_call\` - Tool execution started
- \`tool_result\` - Tool execution completed
- \`approval_required\` - User approval needed`,
    relatedFiles: ['swecli/web/routes/chat.py', 'swecli/web/routes/sessions.py', 'swecli/web/config.py'],
    relatedPages: ['web-interface'],
    tags: ['api', 'reference', 'endpoints', 'websocket']
  }
];

// Mock documentation tree that combines wiki pages with traditional files
const mockDocumentation: DocumentationItem[] = [
  {
    id: 'wiki-overview',
    title: 'Architecture Overview',
    type: 'wiki-page',
    lastModified: '2 hours ago',
    wikiPage: mockWikiPages[0]
  },
  {
    id: 'wiki-agent-system',
    title: 'Agent System',
    type: 'wiki-page',
    lastModified: '3 hours ago',
    wikiPage: mockWikiPages[1]
  },
  {
    id: 'wiki-web-interface',
    title: 'Web Interface',
    type: 'wiki-page',
    lastModified: '1 day ago',
    wikiPage: mockWikiPages[2]
  },
  {
    id: '1',
    title: 'README.md',
    type: 'readme',
    path: '/README.md',
    lastModified: '2 hours ago',
    content: `# OpenDev

Software Engineering CLI with AI-powered coding assistance.

## Features

- **AI-Powered Development**: Get intelligent coding assistance
- **Multi-language Support**: Works with Python, JavaScript, TypeScript, and more
- **Git Integration**: Seamless version control integration
- **Interactive Debugging**: Advanced debugging capabilities

## Quick Start

\`\`\`bash
npm install -g swe-cli
swe-cli init
swe-cli chat
\`\`\`

## Documentation

- [Getting Started](./docs/getting-started.md)
- [API Reference](./docs/api.md)
- [Examples](./examples/)`
  },
  {
    id: 'docs',
    title: 'docs',
    type: 'folder',
    path: '/docs',
    lastModified: '1 day ago',
    children: [
      {
        id: '2-1',
        title: 'getting-started.md',
        type: 'doc',
        path: '/docs/getting-started.md',
        lastModified: '1 day ago'
      },
      {
        id: '2-2',
        title: 'api.md',
        type: 'doc',
        path: '/docs/api.md',
        lastModified: '3 days ago'
      }
    ]
  }
];

export function DocumentationViewer({ selectedRepo, searchQuery, onIndexingChange }: DocumentationViewerProps) {
  const [selectedDoc, setSelectedDoc] = useState<DocumentationItem | null>(null);
  const [chatMode, setChatMode] = useState<'browse' | 'chat'>('browse');
  const [chatMessage, setChatMessage] = useState('');
  const [isSearching, setIsSearching] = useState(false);

  // TODO: Implement search and indexing functionality
  void searchQuery;
  void onIndexingChange;
  void isSearching;
  void setIsSearching;

  if (!selectedRepo) {
    return (
      <div className="h-full flex items-center justify-center bg-gray-50">
        <div className="text-center max-w-md mx-auto p-8">
          <div className="w-20 h-20 rounded-full bg-purple-100 flex items-center justify-center mx-auto mb-6">
            <DocumentTextIcon className="w-10 h-10 text-purple-600" />
          </div>
          <h2 className="text-2xl font-bold text-gray-900 mb-4">Welcome to CodeWiki</h2>
          <p className="text-gray-600 mb-6">
            Select a repository from the sidebar to explore its documentation.
            CodeWiki provides AI-powered documentation search and chat capabilities.
          </p>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
            <div className="bg-white p-4 rounded-lg border border-gray-200">
              <div className="flex items-center gap-2 mb-2">
                <MagnifyingGlassIcon className="w-5 h-5 text-purple-600" />
                <h3 className="font-semibold text-gray-900">Smart Search</h3>
              </div>
              <p className="text-gray-600 text-xs">Find information across all documentation files</p>
            </div>
            <div className="bg-white p-4 rounded-lg border border-gray-200">
              <div className="flex items-center gap-2 mb-2">
                <ChatBubbleLeftRightIcon className="w-5 h-5 text-purple-600" />
                <h3 className="font-semibold text-gray-900">AI Chat</h3>
              </div>
              <p className="text-gray-600 text-xs">Ask questions about code and documentation</p>
            </div>
            <div className="bg-white p-4 rounded-lg border border-gray-200">
              <div className="flex items-center gap-2 mb-2">
                <SparklesIcon className="w-5 h-5 text-purple-600" />
                <h3 className="font-semibold text-gray-900">Context Aware</h3>
              </div>
              <p className="text-gray-600 text-xs">Understands your codebase structure and context</p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  const renderDocumentationItem = (item: DocumentationItem, level = 0) => {
    const getItemIcon = () => {
      switch (item.type) {
        case 'wiki-page':
          return item.wikiPage ? (
            <div className="flex items-center gap-1">
              <DocumentTextIcon className="w-4 h-4 text-purple-500" />
              {item.wikiPage.importance && item.wikiPage.importance > 8 && (
                <div className="w-2 h-2 bg-orange-400 rounded-full" />
              )}
            </div>
          ) : <DocumentTextIcon className="w-4 h-4 text-purple-500" />;
        case 'readme':
          return <DocumentTextIcon className="w-4 h-4 text-blue-500" />;
        case 'doc':
          return <DocumentTextIcon className="w-4 h-4 text-gray-500" />;
        case 'code':
          return <CodeBracketIcon className="w-4 h-4 text-green-500" />;
        case 'folder':
          return <FolderIcon className="w-4 h-4 text-yellow-500" />;
      }
    };

    return (
      <div key={item.id}>
        <div
          className={`flex items-center gap-2 px-3 py-2 hover:bg-gray-50 rounded-lg cursor-pointer transition-colors ${
            selectedDoc?.id === item.id ? 'bg-purple-50 border-l-2 border-purple-500' : ''
          }`}
          style={{ paddingLeft: `${12 + level * 16}px` }}
          onClick={() => setSelectedDoc(item)}
        >
          {getItemIcon()}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-sm text-gray-900 truncate">{item.title}</span>
              {item.wikiPage?.tags && (
                <div className="flex gap-1">
                  {item.wikiPage.tags.slice(0, 2).map(tag => (
                    <span
                      key={tag}
                      className="px-2 py-0.5 text-xs bg-purple-100 text-purple-700 rounded-full"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              )}
            </div>
            {item.wikiPage?.description && (
              <p className="text-xs text-gray-600 mt-1 line-clamp-2">{item.wikiPage.description}</p>
            )}
          </div>
          <div className="flex flex-col items-end gap-1">
            <span className="text-xs text-gray-500">{item.lastModified}</span>
            {item.wikiPage?.relatedFiles && (
              <span className="text-xs text-blue-600">
                {item.wikiPage.relatedFiles.length} files
              </span>
            )}
          </div>
        </div>
        {item.children && item.children.map(child => renderDocumentationItem(child, level + 1))}
      </div>
    );
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="border-b border-gray-200 bg-white">
        <div className="flex items-center justify-between p-4">
          <div>
            <h1 className="text-xl font-bold text-gray-900">CodeWiki</h1>
            <p className="text-sm text-gray-600">Repository Documentation Explorer</p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setChatMode(chatMode === 'browse' ? 'chat' : 'browse')}
              className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors flex items-center gap-2 ${
                chatMode === 'chat'
                  ? 'bg-purple-600 text-white'
                  : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
              }`}
            >
              {chatMode === 'chat' ? (
                <>
                  <DocumentTextIcon className="w-4 h-4" />
                  Browse
                </>
              ) : (
                <>
                  <ChatBubbleLeftRightIcon className="w-4 h-4" />
                  Chat
                </>
              )}
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden">
        {/* Documentation Tree */}
        <div className="w-80 border-r border-gray-200 bg-gray-50 overflow-y-auto">
          <div className="p-4">
            <h3 className="text-sm font-semibold text-gray-900 mb-3">Documentation</h3>
            <div className="space-y-1">
              {mockDocumentation.map(item => renderDocumentationItem(item))}
            </div>
          </div>
        </div>

        {/* Content Area */}
        <div className="flex-1 flex flex-col bg-white">
          {selectedDoc ? (
            <>
              {/* Document Header */}
              <div className="border-b border-gray-200 p-4">
                <div className="flex items-start gap-3 mb-3">
                  <div>
                    {selectedDoc.type === 'wiki-page' && <DocumentTextIcon className="w-5 h-5 text-purple-500" />}
                    {selectedDoc.type === 'readme' && <DocumentTextIcon className="w-5 h-5 text-blue-500" />}
                    {selectedDoc.type === 'doc' && <DocumentTextIcon className="w-5 h-5 text-gray-500" />}
                    {selectedDoc.type === 'code' && <CodeBracketIcon className="w-5 h-5 text-green-500" />}
                  </div>
                  <div className="flex-1">
                    <h2 className="text-lg font-semibold text-gray-900 mb-1">{selectedDoc.title}</h2>
                    {selectedDoc.wikiPage && (
                      <div className="flex items-center gap-3 text-sm text-gray-600">
                        <span className="px-2 py-1 bg-purple-100 text-purple-700 rounded-full text-xs font-medium">
                          {selectedDoc.wikiPage.type}
                        </span>
                        {selectedDoc.wikiPage.importance && (
                          <span className="flex items-center gap-1">
                            <span>Importance:</span>
                            <span className="font-medium">{selectedDoc.wikiPage.importance}/10</span>
                          </span>
                        )}
                        {selectedDoc.wikiPage.lastModified && (
                          <span>Last updated: {selectedDoc.wikiPage.lastModified}</span>
                        )}
                      </div>
                    )}
                    {selectedDoc.path && (
                      <p className="text-sm text-gray-600 font-mono">{selectedDoc.path}</p>
                    )}
                  </div>
                </div>
                {selectedDoc.wikiPage?.description && (
                  <p className="text-sm text-gray-700 bg-purple-50 p-3 rounded-lg border border-purple-200">
                    {selectedDoc.wikiPage.description}
                  </p>
                )}
                {selectedDoc.wikiPage?.tags && (
                  <div className="flex gap-2 flex-wrap">
                    {selectedDoc.wikiPage.tags.map(tag => (
                      <span
                        key={tag}
                        className="px-3 py-1 bg-purple-100 text-purple-700 rounded-full text-xs font-medium"
                      >
                        #{tag}
                      </span>
                    ))}
                  </div>
                )}
              </div>

              {/* Document Content */}
              <div className="flex-1 overflow-y-auto p-6">
                {selectedDoc.wikiPage ? (
                  <div className="prose prose-sm max-w-none">
                    <div dangerouslySetInnerHTML={{
                      __html: selectedDoc.wikiPage.content.replace(
                        /`([^`]+)`/g,
                        '<code class="bg-gray-100 px-1 py-0.5 rounded text-sm text-gray-800">$1</code>'
                      ).replace(
                        /\*\*([^*]+)\*\*/g,
                        '<strong>$1</strong>'
                      ).replace(
                        /### (.+)/g,
                        '<h3 class="text-lg font-semibold text-gray-900 mt-6 mb-3">$1</h3>'
                      ).replace(
                        /## (.+)/g,
                        '<h2 class="text-xl font-bold text-gray-900 mt-8 mb-4">$1</h2>'
                      ).replace(
                        /# (.+)/g,
                        '<h1 class="text-2xl font-bold text-gray-900 mt-8 mb-6">$1</h1>'
                      ).replace(
                        /- (.+)/g,
                        '<li class="ml-4 mb-1">$1</li>'
                      ).replace(
                        /\n\n/g,
                        '</p><p class="mb-4">'
                      )
                    }} />
                    {selectedDoc.wikiPage.relatedFiles && selectedDoc.wikiPage.relatedFiles.length > 0 && (
                      <div className="mt-8 p-4 bg-blue-50 rounded-lg border border-blue-200">
                        <h4 className="font-semibold text-blue-900 mb-2">Related Files</h4>
                        <div className="space-y-2">
                          {selectedDoc.wikiPage.relatedFiles.map((file, index) => (
                            <div key={index} className="flex items-center gap-2 text-sm">
                              <CodeBracketIcon className="w-4 h-4 text-blue-600" />
                              <code className="text-blue-800 bg-blue-100 px-2 py-1 rounded">{file}</code>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                    {selectedDoc.wikiPage.relatedPages && selectedDoc.wikiPage.relatedPages.length > 0 && (
                      <div className="mt-8 p-4 bg-purple-50 rounded-lg border border-purple-200">
                        <h4 className="font-semibold text-purple-900 mb-2">Related Pages</h4>
                        <div className="space-y-2">
                          {selectedDoc.wikiPage.relatedPages.map((page, index) => (
                            <div key={index} className="flex items-center gap-2 text-sm">
                              <DocumentTextIcon className="w-4 h-4 text-purple-600" />
                              <span className="text-purple-800 underline cursor-pointer hover:text-purple-900">{page}</span>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                    {selectedDoc.wikiPage.diagrams && selectedDoc.wikiPage.diagrams.length > 0 && (
                      <div className="mt-8 p-4 bg-green-50 rounded-lg border border-green-200">
                        <h4 className="font-semibold text-green-900 mb-2">Diagrams</h4>
                        <div className="space-y-2 text-sm text-green-800">
                          {selectedDoc.wikiPage.diagrams.map((diagram, index) => (
                            <div key={index} className="flex items-center gap-2">
                              📊 {diagram}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                ) : selectedDoc.content ? (
                  <div className="prose prose-sm max-w-none">
                    <pre className="whitespace-pre-wrap text-sm text-gray-800 font-mono">
                      {selectedDoc.content}
                    </pre>
                  </div>
                ) : (
                  <div className="flex flex-col items-center justify-center h-full text-center">
                    <DocumentTextIcon className="w-12 h-12 text-gray-400 mb-4" />
                    <p className="text-gray-600">Content preview not available</p>
                    <p className="text-sm text-gray-500 mt-2">File last modified: {selectedDoc.lastModified}</p>
                  </div>
                )}
              </div>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center">
              <div className="text-center">
                <DocumentTextIcon className="w-16 h-16 text-gray-400 mx-auto mb-4" />
                <h3 className="text-lg font-medium text-gray-900 mb-2">Select a document</h3>
                <p className="text-gray-600">Choose a file from the documentation tree to view its content</p>
              </div>
            </div>
          )}

          {/* Chat Interface (when in chat mode) */}
          {chatMode && (
            <div className="border-t border-gray-200 p-4">
              <div className="flex items-center gap-3">
                <ChatBubbleLeftRightIcon className="w-5 h-5 text-purple-600" />
                <input
                  type="text"
                  placeholder="Ask about this repository..."
                  value={chatMessage}
                  onChange={(e) => setChatMessage(e.target.value)}
                  className="flex-1 px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-transparent text-sm"
                />
                <button className="px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white text-sm font-medium rounded-lg transition-colors flex items-center gap-2">
                  <SparklesIcon className="w-4 h-4" />
                  Ask
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}