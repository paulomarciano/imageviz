import { type ReactNode, useEffect, useRef, useCallback } from 'react';
import { useDrag } from 'react-dnd';
import type { MediaItem } from '../../types/media';

interface DragSourceProps {
  readonly item: MediaItem;
  readonly children: ReactNode;
}

export const DRAG_TYPE = 'MEDIA_ITEM';

interface DragItem {
  type: typeof DRAG_TYPE;
  id: string;
  filename: string;
  fileUrl: string;
  thumbnailUrl: string;
  mimeType: string;
}

export function DragSource({ item, children }: DragSourceProps) {
  const elRef = useRef<HTMLDivElement>(null);
  const blobRef = useRef<Blob | null>(null);

  const [{ isDragging }, drag] = useDrag<DragItem, void, { isDragging: boolean }>(
    () => ({
      type: DRAG_TYPE,
      item: {
        type: DRAG_TYPE,
        id: item.id,
        filename: item.filename,
        fileUrl: `/api/v1/media/${item.id}/file`,
        thumbnailUrl: item.thumbnail_url,
        mimeType: item.mime_type,
      },
      collect: (monitor) => ({
        isDragging: monitor.isDragging(),
      }),
    }),
    [item],
  );

  // Combine refs: connect react-dnd drag source and capture element for native events
  const setRef = useCallback(
    (node: HTMLDivElement | null) => {
      drag(node);
      elRef.current = node;
    },
    [drag],
  );

  // Pre-fetch the file blob when the thumbnail becomes visible so it's
  // available synchronously during the dragstart event.  The DataTransfer
  // API only accepts new items during the dragstart handler — async
  // additions are ignored by the browser.
  useEffect(() => {
    const fileUrl = `/api/v1/media/${item.id}/file`;
    const absoluteUrl = `${window.location.origin}${fileUrl}`;
    let cancelled = false;

    fetch(absoluteUrl)
      .then((res) => (res.ok ? res.blob() : null))
      .then((blob) => {
        if (!cancelled && blob) {
          blobRef.current = blob;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      blobRef.current = null;
    };
  }, [item.id]);

  // Set native drag data for OS-level drops
  useEffect(() => {
    const el = elRef.current;
    if (!el) return;

    const handleDragStart = (e: DragEvent) => {
      const fileUrl = `/api/v1/media/${item.id}/file`;
      const absoluteUrl = `${window.location.origin}${fileUrl}`;

      // Set URL-based data as synchronous fallback (works for apps that
      // accept URL drops, e.g. browser tabs).
      e.dataTransfer!.setData('text/uri-list', absoluteUrl);
      e.dataTransfer!.setData('text/plain', absoluteUrl);
      e.dataTransfer!.effectAllowed = 'copy';

      // Set drag preview image from the thumbnail.
      const img = new Image();
      img.src = item.thumbnail_url;
      e.dataTransfer!.setDragImage(img, 50, 50);

      // Add the pre-fetched file blob as a native File object synchronously.
      // This makes external apps (Discord, image editors, file manager)
      // see the drop as a real file in dataTransfer.files — equivalent to
      // dragging from the file manager.
      const blob = blobRef.current;
      if (blob) {
        const file = new File([blob], item.filename, { type: item.mime_type });
        e.dataTransfer!.items.add(file);
      }
    };

    el.addEventListener('dragstart', handleDragStart);
    return () => el.removeEventListener('dragstart', handleDragStart);
  }, [item]);

  return (
    <div
      ref={setRef}
      style={{ opacity: isDragging ? 0.4 : 1 }}
      className="cursor-grab active:cursor-grabbing"
    >
      {children}
    </div>
  );
}
