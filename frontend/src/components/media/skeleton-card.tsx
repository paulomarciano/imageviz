import { Skeleton } from '../shared/skeleton';

export function SkeletonCard() {
  return (
    <div className="rounded-lg overflow-hidden bg-gray-800 border border-gray-700" aria-busy="true">
      <div className="aspect-[3/4] bg-gray-700 animate-pulse" />
      <div className="p-2 space-y-2">
        <Skeleton className="h-3 w-3/4" />
        <Skeleton className="h-2 w-1/2" />
      </div>
    </div>
  );
}
