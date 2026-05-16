# Wave 6.3 — Build Configuration Panel (Folder Picker + List)

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 03 |
| **Estimate** | 2.5 hours |
| **Depends on** | 4.2 (API client) |
| **Parallel** | No |

---

## Overview

Build a configuration panel where users can add and remove watched folders. Shows the current list of watched folders with their labels, indexing status, and last indexed timestamp. Includes a folder path input with server-side path suggestions.

## Prerequisites

- API client (4.2)
- Config endpoints: GET /config, PUT /config (1.2)
- Stats endpoint: GET /stats (3.8)
- App shell (4.5)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET/PUT /config), §10.Q4 (multiple folders, track subfolders)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/config/
├── config-panel.tsx             # Configuration panel component
└── __tests__/
    └── config-panel.test.tsx    # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Panel opens as a slide-out or modal from the header
- [ ] Displays list of current watched folders with label and path
- [ ] "Add folder" button with path input field
- [ ] Remove button (×) on each folder to unwatch
- [ ] Save button to persist changes (PUT /config)
- [ ] Shows indexing status from GET /stats (total files, by type, last indexed)
- [ ] Shows "Index now" button to trigger re-indexing
- [ ] Validation: empty paths are rejected, paths must exist (optional — warn if not)
- [ ] Loading spinner while fetching/saving config
- [ ] Error display if API fails
- [ ] Close button (× or Escape)

## Implementation Notes

```tsx
import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import type { AppConfig, WatchedFolder, IndexStats } from '../../types/api';

export function ConfigPanel({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [localFolders, setLocalFolders] = useState<WatchedFolder[]>([]);
  const [newPath, setNewPath] = useState('');
  const [newLabel, setNewLabel] = useState('');

  // Fetch current config
  const configQuery = useQuery({
    queryKey: ['config'],
    queryFn: fetchConfig,
    onSuccess: (data) => {
      setLocalFolders(data.watched_folders);
    },
  });

  // Fetch stats
  const statsQuery = useQuery({
    queryKey: ['stats'],
    queryFn: fetchStats,
  });

  // Save mutation
  const saveMutation = useMutation({
    mutationFn: (config: AppConfig) => saveConfig(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['config'] });
      queryClient.invalidateQueries({ queryKey: ['stats'] });
    },
  });

  const addFolder = useCallback(() => {
    if (!newPath.trim()) return;
    setLocalFolders((prev) => [
      ...prev,
      { path: newPath.trim(), label: newLabel.trim() || undefined },
    ]);
    setNewPath('');
    setNewLabel('');
  }, [newPath, newLabel]);

  const removeFolder = useCallback((index: number) => {
    setLocalFolders((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const handleSave = useCallback(() => {
    saveMutation.mutate({ watched_folders: localFolders });
  }, [localFolders, saveMutation]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') onClose();
  }, [onClose]);

  return (
    <div className="fixed inset-y-0 right-0 w-96 bg-gray-850 border-l border-gray-700 shadow-2xl z-40 flex flex-col"
         onKeyDown={handleKeyDown}>
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-gray-700">
        <h2 className="text-lg font-semibold text-white">Settings</h2>
        <button onClick={onClose} className="text-gray-400 hover:text-white" aria-label="Close settings">✕</button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {/* Watched folders section */}
        <section>
          <h3 className="text-sm font-medium text-gray-300 mb-2">Watched Folders</h3>
          
          {/* Add new folder */}
          <div className="flex gap-2 mb-3">
            <input
              type="text"
              placeholder="Folder path (e.g., ~/ComfyUI/output)"
              value={newPath}
              onChange={(e) => setNewPath(e.target.value)}
              className="flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded text-sm text-white 
                         placeholder-gray-400 focus:outline-none focus:border-blue-500"
            />
            <input
              type="text"
              placeholder="Label (optional)"
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              className="w-24 px-2 py-1.5 bg-gray-700 border border-gray-600 rounded text-sm text-white 
                         placeholder-gray-400 focus:outline-none focus:border-blue-500"
            />
            <button
              onClick={addFolder}
              disabled={!newPath.trim()}
              className="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed 
                         rounded text-sm text-white transition-colors"
            >
              Add
            </button>
          </div>

          {/* Folder list */}
          {localFolders.length === 0 ? (
            <p className="text-gray-500 text-sm">No folders configured. Add a folder to start indexing.</p>
          ) : (
            <ul className="space-y-2">
              {localFolders.map((folder, i) => (
                <li key={i} className="flex items-center justify-between p-2 bg-gray-800 rounded">
                  <div className="min-w-0">
                    <p className="text-sm text-white truncate">{folder.label || folder.path}</p>
                    <p className="text-xs text-gray-400 truncate">{folder.path}</p>
                  </div>
                  <button
                    onClick={() => removeFolder(i)}
                    className="ml-2 p-1 text-gray-500 hover:text-red-400 shrink-0"
                    aria-label={`Remove ${folder.path}`}
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* Stats section */}
        <section>
          <h3 className="text-sm font-medium text-gray-300 mb-2">Index Statistics</h3>
          {statsQuery.isLoading ? (
            <p className="text-gray-500 text-sm">Loading...</p>
          ) : statsQuery.data ? (
            <div className="bg-gray-800 rounded p-3 space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-gray-400">Total files</span>
                <span className="text-white">{statsQuery.data.total_files.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Total size</span>
                <span className="text-white">{formatBytes(statsQuery.data.total_size_bytes)}</span>
              </div>
              {statsQuery.data.by_mime_type && Object.entries(statsQuery.data.by_mime_type).map(([type, count]) => (
                <div key={type} className="flex justify-between">
                  <span className="text-gray-400">{type}</span>
                  <span className="text-white">{count.toLocaleString()}</span>
                </div>
              ))}
              {statsQuery.data.last_indexed_at && (
                <div className="flex justify-between">
                  <span className="text-gray-400">Last indexed</span>
                  <span className="text-white text-xs">{new Date(statsQuery.data.last_indexed_at).toLocaleString()}</span>
                </div>
              )}
            </div>
          ) : null}
        </section>
      </div>

      {/* Footer */}
      <div className="p-4 border-t border-gray-700 flex gap-2">
        <button
          onClick={handleSave}
          disabled={saveMutation.isPending}
          className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded text-sm text-white 
                     transition-colors"
        >
          {saveMutation.isPending ? 'Saving...' : 'Save'}
        </button>
        <button
          onClick={onClose}
          className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm text-white transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
```

**Trigger:** Add a settings icon/button to the header that toggles this panel open.

## Test Strategy

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ConfigPanel } from '../config-panel';

describe('ConfigPanel', () => {
  it('renders watched folders list', async () => {
    renderWithProviders(<ConfigPanel onClose={vi.fn()} />);
    
    await waitFor(() => {
      expect(screen.getByText('Watched Folders')).toBeInTheDocument();
    });
  });

  it('adds a folder', async () => {
    renderWithProviders(<ConfigPanel onClose={vi.fn()} />);
    
    const input = screen.getByPlaceholderText(/Folder path/);
    await userEvent.type(input, '/tmp/images');
    await userEvent.click(screen.getByText('Add'));
    
    expect(screen.getByText('/tmp/images')).toBeInTheDocument();
  });

  it('removes a folder', async () => {
    // Pre-populate with a folder, click remove
  });

  it('saves config on Save click', async () => {
    // Add a folder, click Save, verify API call
  });

  it('closes on Escape', () => {
    const onClose = vi.fn();
    renderWithProviders(<ConfigPanel onClose={onClose} />);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
```
