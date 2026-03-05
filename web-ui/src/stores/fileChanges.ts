import { create } from 'zustand';
import { apiClient } from '../api/client';

export interface FileChange {
  id: string;
  type: 'created' | 'modified' | 'deleted' | 'renamed';
  file_path: string;
  old_path?: string;
  timestamp: string;
  lines_added: number;
  lines_removed: number;
  description?: string;
  icon: string;
  color: string;
  summary: string;
}

export interface FileChangesSummary {
  total: number;
  created: number;
  modified: number;
  deleted: number;
  renamed: number;
  total_lines_added: number;
  total_lines_removed: number;
  net_lines: number;
}

interface FileChangesState {
  changes: FileChange[];
  summary: FileChangesSummary | null;
  isLoading: boolean;
  error: string | null;

  // Actions
  loadFileChanges: (sessionId: string) => Promise<void>;
  clearChanges: () => void;
}

export const useFileChangesStore = create<FileChangesState>((set) => ({
  changes: [],
  summary: null,
  isLoading: false,
  error: null,

  loadFileChanges: async (sessionId: string) => {
    set({ isLoading: true, error: null });

    try {
      const response = await apiClient.get(`/sessions/${sessionId}/file-changes`);
      const data = response.data;

      set({
        changes: data.changes || [],
        summary: data.summary || null,
        isLoading: false,
      });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : 'Failed to load file changes',
        isLoading: false,
      });
    }
  },

  clearChanges: () => {
    set({
      changes: [],
      summary: null,
      error: null,
    });
  },
}));