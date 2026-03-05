/**
 * File Mention Dropdown Component
 *
 * Shows a dropdown list of files when user types @ in the input box.
 * Supports arrow key navigation and Enter key selection.
 */

import { useEffect, useRef } from 'react';
import { DocumentIcon } from '@heroicons/react/24/outline';

interface FileItem {
  path: string;
  name: string;
  is_file: boolean;
}

interface FileMentionDropdownProps {
  files: FileItem[];
  selectedIndex: number;
  onSelect: (file: FileItem) => void;
  onClose: () => void;
  position: { top: number; left: number };
}

export function FileMentionDropdown({
  files,
  selectedIndex,
  onSelect,
  onClose,
  position
}: FileMentionDropdownProps) {
  const dropdownRef = useRef<HTMLDivElement>(null);
  const selectedItemRef = useRef<HTMLDivElement>(null);

  // Scroll selected item into view
  useEffect(() => {
    if (selectedItemRef.current) {
      selectedItemRef.current.scrollIntoView({
        block: 'nearest',
        behavior: 'smooth'
      });
    }
  }, [selectedIndex]);

  // Handle click outside to close
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  if (files.length === 0) {
    return (
      <div
        ref={dropdownRef}
        className="fixed z-50 bg-white border border-gray-300 rounded-lg shadow-lg"
        style={{ top: position.top, left: position.left }}
      >
        <div className="px-4 py-3 text-sm text-gray-500">
          No files found
        </div>
      </div>
    );
  }

  return (
    <div
      ref={dropdownRef}
      className="fixed z-50 bg-white border border-gray-300 rounded-lg shadow-lg w-96 max-h-64 overflow-y-auto"
      style={{ top: position.top, left: position.left }}
    >
      {files.map((file, index) => (
        <div
          key={file.path}
          ref={index === selectedIndex ? selectedItemRef : null}
          onClick={() => onSelect(file)}
          className={`flex items-center gap-3 px-4 py-2.5 cursor-pointer transition-colors ${
            index === selectedIndex
              ? 'bg-gray-100'
              : 'hover:bg-gray-50'
          }`}
        >
          <DocumentIcon className="w-4 h-4 text-gray-400 flex-shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium text-gray-900 truncate">
              {file.name}
            </div>
            <div className="text-xs text-gray-500 truncate">
              {file.path}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
