import { SkeletonCard } from './skeleton-card';

export function SkeletonGrid() {
  return (
    <div aria-busy="true" aria-label="Loading media" className="p-3">
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3">
        {Array.from({ length: 15 }, (_, i) => (
          <SkeletonCard key={i} />
        ))}
      </div>
    </div>
  );
}
