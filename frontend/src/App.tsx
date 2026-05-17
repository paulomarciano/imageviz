import { lazy, Suspense, useCallback, useEffect, useMemo } from 'react';
import { useAtom, useAtomValue } from 'jotai';
import { useQueryClient } from '@tanstack/react-query';
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

function App() {
  const queryClient = useQueryClient();
  const searchQuery = useAtomValue(searchQueryAtom);
  const viewMode = useAtomValue(mediaViewModeAtom);
  const [selectedItem, setSelectedItem] = useAtom(selectedMediaItemAtom);
  const [detailOpen, setDetailOpen] = useAtom(detailViewOpenAtom);
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

  const browseData = useInfiniteMedia(100, undefined, viewMode !== 'search');
  const searchData = useSearch(searchQuery, 100, undefined, 'recency');
  const allItems = viewMode === 'search' ? searchData.results : browseData.allItems;

  const handleItemClick = useCallback(
    (item: MediaItem) => {
      setSelectedItem(item);
      setDetailOpen(true);
    },
    [setSelectedItem, setDetailOpen],
  );

  const currentIndex = useMemo(
    () => (selectedItem ? allItems.findIndex((i) => i.id === selectedItem.id) : -1),
    [selectedItem, allItems],
  );

  // When the clicked item is found in allItems, use the full list for navigation.
  // When it isn't (race condition between infinite-scroll pages), fall back to a
  // single-item list so the detail view still opens.
  const detailItems = useMemo(
    () => (currentIndex >= 0 ? allItems : selectedItem ? [selectedItem] : []),
    [currentIndex, allItems, selectedItem],
  );
  const detailIndex = currentIndex >= 0 ? currentIndex : 0;

  const handleNavigate = useCallback(
    (index: number) => {
      const item = detailItems[index];
      if (item) {
        setSelectedItem(item);
      }
    },
    [detailItems, setSelectedItem],
  );

  const handleClose = useCallback(() => {
    setDetailOpen(false);
    if (selectedItem) {
      queryClient.removeQueries({ queryKey: ['media', 'item', selectedItem.id] });
    }
    setSelectedItem(null);
  }, [setDetailOpen, setSelectedItem, selectedItem, queryClient]);

  return (
    <DndProvider backend={HTML5Backend}>
      <AppShell>
        <ThumbnailGrid onItemClick={handleItemClick} />
      </AppShell>
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
      <ShortcutsPanel isOpen={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      <Suspense fallback={null}>
        {configOpen && <ConfigPanel onClose={() => setConfigOpen(false)} />}
      </Suspense>
    </DndProvider>
  );
}

export default App;
