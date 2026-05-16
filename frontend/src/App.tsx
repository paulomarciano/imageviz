import { useSetAtom } from 'jotai';
import { DndProvider } from 'react-dnd';
import { HTML5Backend } from 'react-dnd-html5-backend';
import { AppShell } from './components/layout/app-shell';
import { ThumbnailGrid } from './components/media/thumbnail-grid';
import { selectedMediaItemAtom, detailViewOpenAtom } from './store/media-atoms';
import type { MediaItem } from './types/media';

function App() {
  const setSelectedItem = useSetAtom(selectedMediaItemAtom);
  const setDetailOpen = useSetAtom(detailViewOpenAtom);

  const handleItemClick = (item: MediaItem) => {
    setSelectedItem(item);
    setDetailOpen(true);
  };

  return (
    <DndProvider backend={HTML5Backend}>
      <AppShell>
        <ThumbnailGrid onItemClick={handleItemClick} />
      </AppShell>
    </DndProvider>
  );
}

export default App;
