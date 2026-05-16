import { useAtom } from 'jotai';
import { searchSortAtom, type SearchSort } from '../../store/search-atoms';

const OPTIONS: { value: SearchSort; label: string }[] = [
  { value: 'recency', label: 'Newest' },
  { value: 'score', label: 'Relevance' },
];

export function SortToggle() {
  const [sort, setSort] = useAtom(searchSortAtom);

  return (
    <div
      className="flex rounded-md overflow-hidden border border-gray-600"
      role="radiogroup"
      aria-label="Search sort order"
    >
      {OPTIONS.map(({ value, label }) => (
        <button
          key={value}
          role="radio"
          aria-checked={sort === value}
          onClick={() => setSort(value)}
          className={`px-3 py-1 text-xs font-medium transition-colors
            ${
              sort === value
                ? 'bg-blue-600 text-white'
                : 'bg-gray-700 text-gray-300 hover:bg-gray-600 hover:text-white'
            }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
