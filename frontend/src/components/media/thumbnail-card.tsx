import { memo, useState, type KeyboardEvent } from 'react';
import type { MediaItem } from '../../types/media';

interface ThumbnailCardProps {
  readonly item: MediaItem;
  readonly onClick: (item: MediaItem) => void;
}

function formatFileSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}

export const ThumbnailCard = memo(function ThumbnailCard({ item, onClick }: ThumbnailCardProps) {
  const [imageLoaded, setImageLoaded] = useState(false);
  const [imageError, setImageError] = useState(false);

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Enter') onClick(item);
  };

  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`View ${item.filename}`}
      className="group cursor-pointer rounded-lg overflow-hidden bg-gray-800 border border-gray-700
                 hover:border-gray-500 hover:scale-[1.02] transition-all duration-150 focus:outline-none
                 focus:ring-2 focus:ring-blue-500"
      onClick={() => onClick(item)}
      onKeyDown={handleKeyDown}
    >
      {/* Image container */}
      <div className="relative aspect-[3/4] bg-gray-700 overflow-hidden">
        {!imageError ? (
          <>
            {/* Skeleton overlay */}
            {!imageLoaded && (
              <div className="absolute inset-0 bg-gray-700 animate-pulse" />
            )}
            <img
              src={item.thumbnail_url}
              alt={item.filename}
              loading="lazy"
              onLoad={() => setImageLoaded(true)}
              onError={() => setImageError(true)}
              className={`w-full h-full object-cover transition-opacity duration-300 ${
                imageLoaded ? 'opacity-100' : 'opacity-0'
              }`}
            />
          </>
        ) : (
          /* Broken thumbnail fallback */
          <div className="flex items-center justify-center h-full text-gray-500">
            <div className="text-center">
              <svg className="w-8 h-8 mx-auto mb-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
                  d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
              <p className="text-xs">No preview</p>
            </div>
          </div>
        )}
      </div>

      {/* Info bar */}
      <div className="p-2 text-xs">
        <p className="truncate text-gray-200 font-medium">{item.filename}</p>
        <p className="text-gray-400">
          {item.width && item.height ? `${item.width}\u00D7${item.height}` : 'Unknown'}
          <span className="mx-1">·</span>
          {formatFileSize(item.file_size)}
        </p>
      </div>
    </div>
  );
});
