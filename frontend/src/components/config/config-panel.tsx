import { useEffect, useRef, useState } from 'react';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get } from '@/api/client';
import type { AppConfig, WatchedFolder, IndexStats } from '@/types/api';
import { CloseIcon, TrashIcon } from '@/components/shared/icons';

/** Format bytes to human-readable string. */
function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}

async function fetchConfig(): Promise<AppConfig> {
  return get<AppConfig>('/config');
}

async function fetchStats(): Promise<IndexStats> {
  return get<IndexStats>('/stats');
}

async function saveConfig(config: AppConfig): Promise<AppConfig> {
  const response = await fetch('/api/v1/config', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  });
  if (!response.ok) {
    throw new Error(`Failed to save config: ${response.statusText}`);
  }
  return response.json() as Promise<AppConfig>;
}

interface ConfigPanelProps {
  readonly onClose: () => void;
}

export function ConfigPanel({ onClose }: ConfigPanelProps) {
  const queryClient = useQueryClient();
  const panelRef = useRef<HTMLDivElement>(null);
  const [localFolders, setLocalFolders] = useState<WatchedFolder[]>([]);
  const [newPath, setNewPath] = useState('');
  const [newLabel, setNewLabel] = useState('');
  const [initialized, setInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useFocusTrap(panelRef);

  // Close panel on Escape key.
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  // Fetch current config
  const configQuery = useQuery<AppConfig, Error>({
    queryKey: ['config'],
    queryFn: fetchConfig,
  });

  // Fetch stats
  const statsQuery = useQuery<IndexStats, Error>({
    queryKey: ['stats'],
    queryFn: fetchStats,
    refetchInterval: 5_000, // Poll every 5s for live updates
  });

  // Initialize local state from fetched config
  useEffect(() => {
    if (configQuery.data && !initialized) {
      setLocalFolders(configQuery.data.watched_folders);
      setInitialized(true);
    }
  }, [configQuery.data, initialized]);

  // Save mutation
  const saveMutation = useMutation<AppConfig, Error, AppConfig>({
    mutationFn: saveConfig,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['config'] });
      void queryClient.invalidateQueries({ queryKey: ['stats'] });
      queryClient.removeQueries({ queryKey: ['media', 'list'] });
      setError(null);
    },
    onError: (err) => {
      setError(err.message);
    },
  });

  const addFolder = () => {
    if (!newPath.trim()) return;
    setLocalFolders((prev) => [
      ...prev,
      { path: newPath.trim(), label: newLabel.trim() || undefined },
    ]);
    setNewPath('');
    setNewLabel('');
  };

  const removeFolder = (index: number) => {
    setLocalFolders((prev) => prev.filter((_, i) => i !== index));
  };

  const handleSave = () => {
    setError(null);
    saveMutation.mutate({ watched_folders: localFolders });
  };

  return (
    <div
      ref={panelRef}
      className="fixed inset-y-0 right-0 w-96 bg-gray-900 border-l border-gray-700 shadow-2xl z-40 flex flex-col"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
    >
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-gray-700">
        <h2 className="text-lg font-semibold text-white">Settings</h2>
        <button
          onClick={onClose}
          className="p-1 text-gray-400 hover:text-white transition-colors rounded"
          aria-label="Close settings"
        >
          <CloseIcon />
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {/* Error display */}
        {error && (
          <div className="bg-red-900/50 border border-red-800 rounded p-3 text-sm text-red-300">
            {error}
          </div>
        )}

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
              onKeyDown={(e) => {
                if (e.key === 'Enter') addFolder();
              }}
              className="flex-1 px-3 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm text-white placeholder-gray-400 focus:outline-none focus:border-blue-500"
              aria-label="Folder path"
            />
            <input
              type="text"
              placeholder="Label"
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') addFolder();
              }}
              className="w-20 px-2 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm text-white placeholder-gray-400 focus:outline-none focus:border-blue-500"
              aria-label="Folder label (optional)"
            />
            <button
              onClick={addFolder}
              disabled={!newPath.trim()}
              className="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed rounded text-sm text-white transition-colors"
            >
              Add
            </button>
          </div>

          {/* Folder list */}
          {localFolders.length === 0 ? (
            <p className="text-gray-500 text-sm">
              No folders configured. Add a folder to start indexing.
            </p>
          ) : (
            <ul className="space-y-2">
              {localFolders.map((folder, i) => (
                <li
                  key={`${folder.path}-${i}`}
                  className="flex items-center justify-between p-2 bg-gray-800 rounded"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-sm text-white truncate">{folder.label || folder.path}</p>
                    {folder.label && (
                      <p className="text-xs text-gray-400 truncate">{folder.path}</p>
                    )}
                  </div>
                  <button
                    onClick={() => removeFolder(i)}
                    className="ml-2 p-1 text-gray-500 hover:text-red-400 shrink-0 transition-colors"
                    aria-label={`Remove ${folder.path}`}
                  >
                    <TrashIcon />
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
                <span className="text-white">{statsQuery.data.total.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Total size</span>
                <span className="text-white">{formatBytes(statsQuery.data.total_file_size)}</span>
              </div>
              {Object.entries(statsQuery.data.by_mime_type).map(([type, count]) => (
                <div key={type} className="flex justify-between">
                  <span className="text-gray-400">{type}</span>
                  <span className="text-white">{count.toLocaleString()}</span>
                </div>
              ))}
              {statsQuery.data.last_indexed_at && (
                <div className="flex justify-between pt-1 border-t border-gray-700">
                  <span className="text-gray-400">Last indexed</span>
                  <span className="text-white text-xs">
                    {new Date(statsQuery.data.last_indexed_at).toLocaleString()}
                  </span>
                </div>
              )}
              <div className="flex justify-between">
                <span className="text-gray-400">Index status</span>
                <span className="text-xs font-medium text-blue-400">
                  {statsQuery.data.indexing.status}
                </span>
              </div>
              {statsQuery.data.indexing.status !== 'Idle' && (
                <div className="flex justify-between">
                  <span className="text-gray-400">Progress</span>
                  <span className="text-white text-xs">
                    {statsQuery.data.indexing.processed} / {statsQuery.data.indexing.total}
                  </span>
                </div>
              )}
            </div>
          ) : statsQuery.isError ? (
            <p className="text-red-400 text-sm">Failed to load stats</p>
          ) : null}
        </section>
      </div>

      {/* Footer */}
      <div className="p-4 border-t border-gray-700 flex gap-2">
        <button
          onClick={handleSave}
          disabled={saveMutation.isPending}
          className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded text-sm text-white transition-colors"
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
