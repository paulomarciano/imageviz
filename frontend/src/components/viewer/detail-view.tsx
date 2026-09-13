/**
 * Detail view modal — full-screen overlay that hosts the image/video viewer
 * and metadata panel. Supports keyboard navigation (← → Escape), body scroll
 * lock, and fade-in entrance animation.
 *
 * The parent provides the current context of `items: readonly MediaItem[]` and
 * the `currentIndex` for arrow-based navigation.
 */

import { useEffect, useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { MediaItem, MediaItemDetail } from '@/types/media';
import { fetchMediaItem } from '@/api/media';
import { formatFileSize } from '@/utils/format';
import { ImageViewer } from './image-viewer';
import { VideoViewer } from './video-viewer';
import { MetadataPanel } from './metadata-panel';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { useEscape } from '@/hooks/use-escape';

interface DetailViewProps {
  readonly items: readonly MediaItem[];
  readonly currentIndex: number;
  readonly onNavigate: (index: number) => void;
  readonly onClose: () => void;
}

export function DetailView({ items, currentIndex, onNavigate, onClose }: DetailViewProps) {
  const overlayRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);

  const item = items[currentIndex];

  const { data: detailItem, isLoading: loadingDetail } = useQuery<MediaItemDetail>({
    queryKey: ['media', 'item', item?.id],
    queryFn: async () => {
      try {
        return await fetchMediaItem(item!.id);
      } catch {
        return {
          ...item!,
          file_url: `/api/v1/media/${item!.id}/file`,
          metadata: null,
        } as MediaItemDetail;
      }
    },
    enabled: !!item,
    staleTime: 5 * 60 * 1000,
    gcTime: 10 * 60 * 1000,
  });

  // Focus the close button on mount (first focusable element).
  useEffect(() => {
    closeButtonRef.current?.focus();
  }, []);

  // Keyboard navigation: ← → navigate (Escape-to-close via useEscape below).
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowLeft':
          if (currentIndex > 0) onNavigate(currentIndex - 1);
          break;
        case 'ArrowRight':
          if (currentIndex < items.length - 1) onNavigate(currentIndex + 1);
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [currentIndex, items.length, onNavigate]);

  // Lock body scroll while the modal is open.
  useEffect(() => {
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = '';
    };
  }, []);

  // Focus trap inside the modal; Escape closes the view.
  useFocusTrap(overlayRef);
  useEscape(onClose);

  // Guard: all hooks must be called before this point.
  if (!item) return null;

  const isVideo = item.mime_type.startsWith('video/');

  return (
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-label="Media detail view"
      className="fixed inset-0 z-50 bg-gray-900 flex animate-fade-in"
    >
      {/* ---- Main viewer area ---- */}
      <div className="flex-1 relative flex items-center justify-center">
        {loadingDetail ? (
          <div className="flex items-center justify-center">
            <div
              aria-label="Loading media details"
              className="animate-spin h-8 w-8 border-2 border-blue-500 border-t-transparent rounded-full"
            />
          </div>
        ) : detailItem ? (
          isVideo ? (
            <VideoViewer item={detailItem} />
          ) : (
            <ImageViewer item={detailItem} />
          )
        ) : null}

        {/* Close button (top-right corner) */}
        <button
          ref={closeButtonRef}
          onClick={onClose}
          className="absolute top-4 right-4 z-10 w-8 h-8 flex items-center justify-center rounded-full
                     bg-black/50 hover:bg-black/70 text-white transition-colors"
          aria-label="Close detail view"
        >
          {'\u2715'}
        </button>

        {/* File info bar (bottom-left) */}
        {detailItem && !loadingDetail && (
          <div className="absolute bottom-4 left-4 z-10 bg-black/70 text-white text-xs px-3 py-1.5 rounded pointer-events-none">
            <span className="font-medium">{item.filename}</span>
            {item.width != null && item.height != null && (
              <span className="ml-2 text-gray-300">
                {item.width}
                {'\u00D7'}
                {item.height}
              </span>
            )}
            <span className="ml-2 text-gray-400">{formatFileSize(item.file_size)}</span>
          </div>
        )}

        {/* Previous arrow (left side) */}
        {currentIndex > 0 && (
          <button
            onClick={() => onNavigate(currentIndex - 1)}
            className="absolute left-4 top-1/2 -translate-y-1/2 z-10 w-10 h-10 flex items-center justify-center
                       rounded-full bg-black/50 hover:bg-black/70 text-white text-xl transition-colors"
            aria-label="Previous item"
          >
            {'\u2190'}
          </button>
        )}

        {/* Next arrow — positioned to avoid overlapping the 320px metadata panel */}
        {currentIndex < items.length - 1 && (
          <button
            onClick={() => onNavigate(currentIndex + 1)}
            className="absolute right-[340px] top-1/2 -translate-y-1/2 z-10 w-10 h-10 flex items-center justify-center
                       rounded-full bg-black/50 hover:bg-black/70 text-white text-xl transition-colors"
            aria-label="Next item"
          >
            {'\u2192'}
          </button>
        )}
      </div>

      {/* ---- Metadata side panel (right side, 320px) ---- */}
      <div className="w-80 shrink-0 bg-gray-900 border-l border-gray-700 overflow-y-auto">
        <MetadataPanel metadata={detailItem?.metadata ?? null} />
      </div>
    </div>
  );
}
