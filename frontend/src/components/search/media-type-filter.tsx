import { useAtom } from 'jotai';
import { mediaTypeFilterAtom, type MediaTypeFilter } from '../../store/search-atoms';

const OPTIONS: { value: MediaTypeFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'image', label: 'Images' },
  { value: 'video', label: 'Videos' },
];

export function MediaTypeFilter() {
  const [filter, setFilter] = useAtom(mediaTypeFilterAtom);

  return (
    <div
      className="flex rounded-md overflow-hidden border border-gray-600"
      role="radiogroup"
      aria-label="Media type filter"
    >
      {OPTIONS.map(({ value, label }) => (
        <button
          key={value}
          role="radio"
          aria-checked={filter === value}
          onClick={() => setFilter(value)}
          className={`px-3 py-1 text-xs font-medium transition-colors
            ${
              filter === value
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
