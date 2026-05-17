import { useAtom } from 'jotai';
import { SearchBar } from '../search/search-bar';
import { MediaTypeFilter } from '../search/media-type-filter';
import { SortToggle } from '../search/sort-toggle';
import { configPanelOpenAtom } from '@/store/ui-atoms';
import { GearIcon } from '@/components/shared/icons';

export function Header() {
  const [, setConfigOpen] = useAtom(configPanelOpenAtom);

  return (
    <header className="h-12 flex items-center justify-between px-4 bg-gray-800 border-b border-gray-700 shrink-0">
      <div className="flex items-center gap-3">
        <h1 className="text-lg font-semibold tracking-tight">ImageViz</h1>
      </div>
      <div className="flex items-center gap-2">
        <MediaTypeFilter />
        <SortToggle />
        <SearchBar />
        <button
          onClick={() => setConfigOpen((prev) => !prev)}
          className="p-1.5 text-gray-400 hover:text-white transition-colors rounded"
          aria-label="Open settings"
        >
          <GearIcon />
        </button>
      </div>
    </header>
  );
}
