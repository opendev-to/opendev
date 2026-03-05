import { useState } from 'react';
import { useParams, Link } from 'react-router-dom';
import { FolderIcon, ArrowLeftIcon } from '@heroicons/react/24/outline';
import { Breadcrumb } from '../Layout/Breadcrumb';
import { WikiSidebar } from './WikiSidebar';
import { DocumentationContent } from './DocumentationContent';
import { Repository } from './RepositoryExplorer';

// Mock function to get repository by name
const getRepositoryByName = (name: string): Repository | null => {
  const mockRepositories: Repository[] = [
    {
      id: '1',
      name: 'react',
      fullName: 'facebook/react',
      description: 'A declarative, efficient, and flexible JavaScript library for building user interfaces.',
      language: 'JavaScript',
      stars: 220000,
      lastIndexed: '2 hours ago',
      status: 'indexed',
      files: 3456,
      docsFound: 234
    },
    {
      id: '2',
      name: 'swe-cli',
      fullName: 'swe-cli/swe-cli',
      description: 'Software Engineering CLI with AI-powered coding assistance.',
      language: 'Python',
      stars: 1250,
      lastIndexed: '1 day ago',
      status: 'indexed',
      files: 892,
      docsFound: 67,
      localPath: '/Users/quocnghi/codes/swe-cli'
    },
    {
      id: '3',
      name: 'next.js',
      fullName: 'vercel/next.js',
      description: 'The React Framework for Production.',
      language: 'TypeScript',
      stars: 125000,
      lastIndexed: '3 days ago',
      status: 'error',
      files: 2341,
      docsFound: 0
    }
  ];

  return mockRepositories.find(repo => repo.name === name) || null;
};

// Mock wiki pages for the repository
const mockWikiPages = [
  {
    id: 'overview',
    title: 'Architecture Overview',
    type: 'overview' as const,
    description: 'High-level architecture and design principles',
    lastModified: '2 hours ago',
    content: `# Architecture Overview

This document provides a comprehensive overview of the repository architecture and design principles.

## Core Components

### 1. Frontend Layer
- React-based UI with TypeScript
- Modern component architecture
- Real-time updates via WebSockets

### 2. Backend Services
- RESTful API endpoints
- WebSocket server for real-time communication
- Database integration with PostgreSQL

### 3. Agent System
- Multi-agent coordination
- Tool execution framework
- Context-aware processing`,
    relatedFiles: ['src/core/agent.py', 'src/web/server.py'],
    relatedPages: ['agent-system', 'api-reference'],
    tags: ['architecture', 'overview', 'core']
  },
  {
    id: 'agent-system',
    title: 'Agent System',
    type: 'architecture' as const,
    parent: 'overview',
    description: 'Detailed breakdown of the agent execution system',
    lastModified: '3 hours ago',
    content: `# Agent System Architecture

The agent system provides intelligent task execution through coordinated multi-agent workflows.

## Agent Types

- **Code Agent**: Handles code generation and modification
- **Debug Agent**: Analyzes and fixes errors
- **Test Agent**: Writes and runs tests
- **Research Agent**: Gathers information`,
    relatedFiles: ['src/agents/base.py', 'src/agents/code.py'],
    relatedPages: ['overview'],
    tags: ['agents', 'architecture']
  },
  {
    id: 'api-reference',
    title: 'API Reference',
    type: 'api' as const,
    description: 'Complete API endpoint documentation',
    lastModified: '1 day ago',
    content: `# API Reference

## REST Endpoints

### Chat API
- \`POST /api/chat/message\` - Send a message
- \`GET /api/chat/history\` - Get chat history

### Sessions API
- \`GET /api/sessions\` - List all sessions
- \`POST /api/sessions\` - Create new session
- \`DELETE /api/sessions/:id\` - Delete session`,
    relatedFiles: ['src/web/routes/chat.py', 'src/web/routes/sessions.py'],
    relatedPages: ['overview'],
    tags: ['api', 'reference', 'endpoints']
  }
];

interface RepositoryDetailPageProps {
  searchQuery?: string;
}

export function RepositoryDetailPage({ searchQuery = '' }: RepositoryDetailPageProps) {
  const { repoName } = useParams<{ repoName: string }>();
  const repository = getRepositoryByName(repoName || '');
  const [selectedPageId, setSelectedPageId] = useState<string | null>('overview');

  const selectedPage = mockWikiPages.find(page => page.id === selectedPageId);

  // TODO: Implement search functionality
  void searchQuery;

  if (!repository) {
    return (
      <div className="min-h-screen bg-gray-50 flex items-center justify-center">
        <div className="text-center">
          <div className="w-20 h-20 mx-auto mb-6 bg-gray-100 rounded-full flex items-center justify-center">
            <FolderIcon className="w-10 h-10 text-gray-400" />
          </div>
          <h3 className="text-xl font-semibold text-gray-900 mb-2">Repository Not Found</h3>
          <p className="text-gray-600 mb-6">The repository "{repoName}" could not be found.</p>
          <Link
            to="/codewiki"
            className="inline-flex items-center gap-2 px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-lg transition-colors"
          >
            <ArrowLeftIcon className="w-4 h-4" />
            Back to CodeWiki
          </Link>
        </div>
      </div>
    );
  }

  // Build breadcrumb items
  const breadcrumbItems = [
    { label: 'CodeWiki', path: '/codewiki' },
    { label: repository.name },
  ];

  if (selectedPage) {
    breadcrumbItems.push({ label: selectedPage.title });
  }

  return (
    <div className="min-h-screen bg-white">
      {/* Breadcrumb Navigation */}
      <Breadcrumb items={breadcrumbItems} />

      {/* 3-Column Layout */}
      <div className="flex">
        {/* Left Sidebar - Wiki Navigation */}
        <WikiSidebar
          wikiPages={mockWikiPages}
          selectedPageId={selectedPageId}
          onPageSelect={setSelectedPageId}
        />

        {/* Main Content Area */}
        <main className="flex-1 bg-white min-h-[calc(100vh-6.5rem)]">
          {selectedPage ? (
            <DocumentationContent wikiPage={selectedPage} />
          ) : (
            <div className="flex items-center justify-center h-96">
              <div className="text-center">
                <FolderIcon className="w-16 h-16 text-gray-400 mx-auto mb-4" />
                <h3 className="text-lg font-medium text-gray-900 mb-2">Select a page</h3>
                <p className="text-gray-600">Choose a wiki page from the sidebar to view its content</p>
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}