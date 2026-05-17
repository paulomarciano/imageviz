import { useState, useCallback, useRef } from 'react';
import { useSetAtom } from 'jotai';
import { searchQueryAtom } from '../../store/search-atoms';
import { useDebounce } from '../../hooks/use-debounce';
import { SearchIcon, CloseIcon } from '../shared/icons';

export function SearchBar() {
  const [localQuery, setLocalQuery] = useState('');
  const setSearchQuery = useSetAtom(searchQueryAtom);
  const inputRef = useRef<HTMLInputElement>(null);

  // Debounce the local query by 300ms before pushing to global state.
  const debouncedQuery = useDebounce(localQuery, 300);
  // Only update the atom when the debounced value changes.
  const prevRef = useRef(debouncedQuery);
  if (debouncedQuery !== prevRef.current) {
    prevRef.current = debouncedQuery;
    setSearchQuery(debouncedQuery);
  }

  const handleClear = useCallback(() => {
    setLocalQuery('');
    setSearchQuery('');
    inputRef.current?.focus();
  }, [setSearchQuery]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        handleClear();
      }
    },
    [handleClear],
  );

  return (
    <div className="relative flex items-center">
      <SearchIcon className="absolute left-3 w-4 h-4 text-gray-400" />
      <input
        ref={inputRef}
        type="search"
        aria-label="Search media"
        autoComplete="off"
        placeholder="Search media..."
        value={localQuery}
        onChange={(e) => setLocalQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        className="w-64 pl-10 pr-8 py-1.5 bg-gray-700 border border-gray-600 rounded-md
                   text-sm text-white placeholder-gray-400
                   focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500
                   transition-colors"
      />
      {localQuery && (
        <button
          onClick={handleClear}
          className="absolute right-2 p-0.5 rounded hover:bg-gray-600 text-gray-400 hover:text-white"
          aria-label="Clear search"
        >
          <CloseIcon className="w-4 h-4" />
        </button>
      )}
    </div>
  );
}
