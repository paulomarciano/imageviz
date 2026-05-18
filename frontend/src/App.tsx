import { lazy, Suspense, useCallback, useEffect, useMemo } from 'react';
import { useAtom, useAtomValue } from 'jotai';
import { DndProvider } from 'react-dnd';
import { HTML5Backend } from 'react-dnd-html5-backend';
import { AppShell } from './components/layout/app-shell';
import { ThumbnailGrid } from './components/media/thumbnail-grid';
const DetailView = lazy(() =>
  import('./components/viewer/detail-view').then((m) => ({ default: m.DetailView })),
);
const ConfigPanel = lazy(() =>
  import('./components/config/config-panel').then((m) => ({ default: m.ConfigPanel })),
);
import { ShortcutsPanel } from './components/shared/shortcuts-panel';
import { selectedMediaItemAtom, detailViewOpenAtom } from './store/media-atoms';
import { searchQueryAtom, mediaViewModeAtom } from './store/search-atoms';
import { shortcutsPanelOpenAtom, configPanelOpenAtom } from './store/ui-atoms';
import { useInfiniteMedia } from './hooks/use-infinite-media';
import { useSearch } from './hooks/use-search';
import { useSseGridUpdates } from './hooks/use-sse-grid-updates';
import type { MediaItem } from './types/media';

/**
 * Inner component that only mounts hooks for the active view mode.
 * Fully unmounted when switching modes, so only one hook observer is alive.
 */
function ActiveViewContent({ onItemClick }: { onItemClick: (item: MediaItem) => void }) {
  const viewMode = useAtomValue(mediaViewModeAtom);
  const searchQuery = useAtomValue(searchQueryAtom);
  const [selectedItem, setSelectedItem] = useAtom(selectedMediaItemAtom);
  const [detailOpen, setDetailOpen] = useAtom(detailViewOpenAtom);

  // Only mount the hook corresponding to the active mode.
  const browseData = useInfiniteMedia(100, undefined, viewMode !== 'search');
  const searchData = useSearch(searchQuery, 100, undefined, 'recency');
  const allItems = viewMode === 'search' ? searchData.results : browseData.allItems;

  const handleItemClick = useCallback(
    (item: MediaItem) => {
      setSelectedItem(item);
      setDetailOpen(true);
      onItemClick(item);
    },
    [setSelectedItem, setDetailOpen, onItemClick],
  );

  const currentIndex = useMemo(
    () => (selectedItem ? allItems.findIndex((i) => i.id === selectedItem.id) : -1),
    [selectedItem, allItems],
  );

  const detailItems = useMemo(
    () => (currentIndex >= 0 ? allItems : selectedItem ? [selectedItem] : []),
    [currentIndex, allItems, selectedItem],
  );
  const detailIndex = currentIndex >= 0 ? currentIndex : 0;

  const handleNavigate = useCallback(
    (index: number) => {
      const item = detailItems[index];
      if (item) setSelectedItem(item);
    },
    [detailItems, setSelectedItem],
  );

  const handleClose = useCallback(() => {
    setDetailOpen(false);
    setSelectedItem(null);
  }, [setDetailOpen, setSelectedItem]);

  return (
    <>
      <ThumbnailGrid onItemClick={handleItemClick} />
      <Suspense fallback={null}>
        {detailOpen && selectedItem && (
          <DetailView
            items={detailItems}
            currentIndex={detailIndex}
            onNavigate={handleNavigate}
            onClose={handleClose}
          />
        )}
      </Suspense>
    </>
  );
}

function App() {
  const [shortcutsOpen, setShortcutsOpen] = useAtom(shortcutsPanelOpenAtom);
  const [configOpen, setConfigOpen] = useAtom(configPanelOpenAtom);

  // Global key handler: ? toggles shortcuts, / focuses search
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = document.activeElement?.tagName;
      const isInput = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';

      if (e.key === '?' && !isInput) {
        e.preventDefault();
        setShortcutsOpen((prev) => !prev);
      }

      if (e.key === '/' && !isInput) {
        e.preventDefault();
        const searchInput = document.querySelector<HTMLInputElement>(
          'input[aria-label="Search media"]',
        );
        searchInput?.focus();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [setShortcutsOpen]);

  // Wire SSE events → TanStack Query cache
  useSseGridUpdates();

  const handleItemClick = useCallback((_item: MediaItem) => {
    // Detail view logic lives in ActiveViewContent
  }, []);

  return (
    <DndProvider backend={HTML5Backend}>
      <AppShell>
        <ActiveViewContent onItemClick={handleItemClick} />
      </AppShell>
      <ShortcutsPanel isOpen={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      <Suspense fallback={null}>
        {configOpen && <ConfigPanel onClose={() => setConfigOpen(false)} />}
      </Suspense>
    </DndProvider>
  );
}

export default App;
