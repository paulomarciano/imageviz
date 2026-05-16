import { useCallback, useMemo } from 'react';
import { useAtom, useAtomValue } from 'jotai';
import { DndProvider } from 'react-dnd';
import { HTML5Backend } from 'react-dnd-html5-backend';
import { AppShell } from './components/layout/app-shell';
import { ThumbnailGrid } from './components/media/thumbnail-grid';
import { DetailView } from './components/viewer/detail-view';
import { selectedMediaItemAtom, detailViewOpenAtom } from './store/media-atoms';
import { searchQueryAtom, mediaViewModeAtom } from './store/search-atoms';
import { useInfiniteMedia } from './hooks/use-infinite-media';
import { useSearch } from './hooks/use-search';
import type { MediaItem } from './types/media';

function App() {
  const searchQuery = useAtomValue(searchQueryAtom);
  const viewMode = useAtomValue(mediaViewModeAtom);
  const [selectedItem, setSelectedItem] = useAtom(selectedMediaItemAtom);
  const [detailOpen, setDetailOpen] = useAtom(detailViewOpenAtom);

  const browseData = useInfiniteMedia();
  const searchData = useSearch(searchQuery);
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

  const handleNavigate = useCallback(
    (index: number) => {
      const item = allItems[index];
      if (item) {
        setSelectedItem(item);
      }
    },
    [allItems, setSelectedItem],
  );

  const handleClose = useCallback(() => {
    setDetailOpen(false);
    setSelectedItem(null);
  }, [setDetailOpen, setSelectedItem]);

  return (
    <DndProvider backend={HTML5Backend}>
      <AppShell>
        <ThumbnailGrid onItemClick={handleItemClick} />
      </AppShell>
      {detailOpen && selectedItem && currentIndex >= 0 && (
        <DetailView
          items={allItems}
          currentIndex={currentIndex}
          onNavigate={handleNavigate}
          onClose={handleClose}
        />
      )}
    </DndProvider>
  );
}

export default App;
