import { memo, useState, type KeyboardEvent } from 'react';
import type { MediaItem } from '../../types/media';
import { formatFileSize } from '../../utils/format';
import { ImageIcon, VideoIcon } from '@/components/shared/icons';

interface ThumbnailCardProps {
  readonly item: MediaItem;
  readonly onClick: (item: MediaItem) => void;
  readonly index?: number;
  readonly isFocused?: boolean;
}

export const ThumbnailCard = memo(function ThumbnailCard({
  item,
  onClick,
  index,
  isFocused = false,
}: ThumbnailCardProps) {
  const [imageStatus, setImageStatus] = useState<'loading' | 'loaded' | 'error'>('loading');

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Enter') onClick(item);
  };

  const handleImageError = () => {
    console.warn('Thumbnail failed to load:', item.thumbnail_url);
    setImageStatus('error');
  };

  const isVideo = item.mime_type.startsWith('video/');

  return (
    <div
      role="button"
      tabIndex={isFocused ? 0 : -1}
      aria-label={`View ${item.filename}`}
      data-grid-index={index}
      className={`group cursor-pointer rounded-lg overflow-hidden bg-gray-800 border border-gray-700
                 hover:border-gray-500 hover:scale-[1.02] transition-all duration-150 focus:outline-none
                 focus:ring-2 ${isFocused ? 'ring-2 ring-blue-500' : 'focus:ring-blue-500'}`}
      onClick={() => onClick(item)}
      onKeyDown={handleKeyDown}
    >
      {/* Image container */}
      <div className="relative aspect-[3/4] bg-gray-700 overflow-hidden">
        {imageStatus === 'loading' && (
          <div className="absolute inset-0 bg-gray-700 animate-pulse" />
        )}

        {imageStatus === 'error' ? (
          /* Graceful placeholder for broken/missing thumbnails */
          <div className="flex flex-col items-center justify-center h-full text-gray-500 px-2">
            {isVideo ? (
              <VideoIcon className="w-10 h-10 mb-2" />
            ) : (
              <ImageIcon className="w-10 h-10 mb-2" />
            )}
            <p className="text-xs text-gray-500 text-center truncate max-w-full">{item.filename}</p>
            <p className="text-[10px] text-gray-600 mt-0.5">Preview unavailable</p>
          </div>
        ) : (
          <img
            src={item.thumbnail_url}
            alt={item.filename}
            loading="lazy"
            decoding="async"
            draggable={false}
            onLoad={() => setImageStatus('loaded')}
            onError={handleImageError}
            className={`w-full h-full object-cover transition-opacity duration-300 ${
              imageStatus === 'loaded' ? 'opacity-100' : 'opacity-0'
            }`}
          />
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
