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

  // Set native drag data for OS-level drops
  useEffect(() => {
    const el = elRef.current;
    if (!el) return;

    const handleDragStart = (e: DragEvent) => {
      const fileUrl = `/api/v1/media/${item.id}/file`;
      const absoluteUrl = `${window.location.origin}${fileUrl}`;
      e.dataTransfer?.setData('text/uri-list', absoluteUrl);
      e.dataTransfer?.setData('text/plain', absoluteUrl);
      e.dataTransfer!.effectAllowed = 'copy';

      const img = new Image();
      img.src = item.thumbnail_url;
      e.dataTransfer?.setDragImage(img, 50, 50);
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
