import { SearchBar } from '../search/search-bar';

export function Header() {
  return (
    <header className="h-12 flex items-center justify-between px-4 bg-gray-800 border-b border-gray-700 shrink-0">
      <div className="flex items-center gap-3">
        <h1 className="text-lg font-semibold tracking-tight">ImageViz</h1>
      </div>
      <div className="flex items-center gap-2">
        <SearchBar />
        {/* Placeholder for config button (Wave 6.3) */}
      </div>
    </header>
  );
}
