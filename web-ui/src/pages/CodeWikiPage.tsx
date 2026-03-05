import { useState } from 'react';
import { RepositoryGrid } from '../components/CodeWiki/RepositoryGrid';
import { Repository } from '../components/CodeWiki/RepositoryExplorer';

// Mock data for repositories
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

export function CodeWikiPage() {
  const [searchQuery, setSearchQuery] = useState('');

  const handleAddRepository = () => {
    // TODO: Implement add repository modal/functionality
    console.log('Add repository clicked');
  };

  return (
    <RepositoryGrid
      repositories={mockRepositories}
      searchQuery={searchQuery}
      onSearchChange={setSearchQuery}
      onAddRepository={handleAddRepository}
    />
  );
}