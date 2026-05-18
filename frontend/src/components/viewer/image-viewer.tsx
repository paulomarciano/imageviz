import { useState, useCallback, useRef, useEffect } from 'react';
import type { MediaItemDetail } from '../../types/media';

interface ImageViewerProps {
  readonly item: MediaItemDetail;
}

/** Minimum interval (ms) between React state syncs for zoom updates. */
const ZOOM_DEBOUNCE_MS = 50;
/** Clamp zoom between 10% and 1000%. */
const ZOOM_MIN = 0.1;
const ZOOM_MAX = 10;

export function ImageViewer({ item }: ImageViewerProps) {
  const [zoom, setZoom] = useState(1);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [imageLoaded, setImageLoaded] = useState(false);
  const [imageError, setImageError] = useState(false);
  const [fitMode, setFitMode] = useState(true);

  // Refs for smooth drag (no re-render on mousemove)
  const containerRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);
  const dragStartRef = useRef({ x: 0, y: 0 });
  const positionRef = useRef({ x: 0, y: 0 });
  // Ref-based zoom tracking — avoids React re-render on every wheel event.
  const zoomRef = useRef(1);
  // Tracks whether we have a pending debounced React state sync.
  const zoomSyncTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Apply current zoom+position as CSS transform directly on the <img>.
  // Called on every wheel/drag event for instant visual feedback.
  const applyTransform = useCallback(() => {
    const img = containerRef.current?.querySelector<HTMLImageElement>('img');
    if (img) {
      img.style.transform = `translate(${positionRef.current.x}px, ${positionRef.current.y}px) scale(${zoomRef.current})`;
    }
  }, []);

  // Debounced sync of zoomRef → React state (for zoom indicator).
  const scheduleZoomSync = useCallback(() => {
    if (zoomSyncTimerRef.current !== null) return; // already pending
    zoomSyncTimerRef.current = setTimeout(() => {
      zoomSyncTimerRef.current = null;
      setZoom(zoomRef.current);
    }, ZOOM_DEBOUNCE_MS);
  }, []);

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      const delta = e.deltaY > 0 ? 0.9 : 1.1;
      const newZoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, zoomRef.current * delta));
      zoomRef.current = newZoom;
      applyTransform();
      scheduleZoomSync();
      setFitMode(false);
    },
    [applyTransform, scheduleZoomSync],
  );

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (zoomRef.current > 1) {
      isDraggingRef.current = true;
      dragStartRef.current = {
        x: e.clientX - positionRef.current.x,
        y: e.clientY - positionRef.current.y,
      };
    }
  }, []);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (!isDraggingRef.current || !containerRef.current) return;

      const newX = e.clientX - dragStartRef.current.x;
      const newY = e.clientY - dragStartRef.current.y;
      positionRef.current = { x: newX, y: newY };

      // Direct DOM manipulation — no React reconciliation on mousemove
      applyTransform();
    },
    [applyTransform],
  );

  const handleMouseUp = useCallback(() => {
    if (isDraggingRef.current) {
      isDraggingRef.current = false;
      // Update React state once at the end of drag
      setPosition({ ...positionRef.current });
    }
  }, []);

  const handleDoubleClick = useCallback(() => {
    if (fitMode) {
      setFitMode(false);
      zoomRef.current = 1;
      setZoom(1);
    } else {
      setFitMode(true);
      zoomRef.current = 1;
      setZoom(1);
      setPosition({ x: 0, y: 0 });
      positionRef.current = { x: 0, y: 0 };
      applyTransform();
    }
  }, [fitMode, applyTransform]);

  const handleRetry = useCallback(() => {
    setImageError(false);
    setImageLoaded(false);
  }, []);

  // Clean up debounce timer on unmount.
  useEffect(() => {
    return () => {
      if (zoomSyncTimerRef.current !== null) {
        clearTimeout(zoomSyncTimerRef.current);
      }
    };
  }, []);

  // Apply initial transform when image loads.
  const handleImageLoad = useCallback(() => {
    setImageLoaded(true);
    applyTransform();
  }, [applyTransform]);

  // Sync React position state to CSS transform when not mid-drag
  const imgStyle = !fitMode
    ? {
        transform: `translate(${position.x}px, ${position.y}px) scale(${zoom})`,
        transformOrigin: 'center center',
      }
    : undefined;

  return (
    <div
      ref={containerRef}
      className="relative w-full h-full flex items-center justify-center bg-black/90 overflow-hidden select-none cursor-grab active:cursor-grabbing"
      onWheel={handleWheel}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onDoubleClick={handleDoubleClick}
    >
      {imageError ? (
        <div className="text-gray-400 text-center">
          <p className="text-lg">Unable to load image</p>
          <button onClick={handleRetry} className="mt-2 text-blue-400 hover:text-blue-300">
            Retry
          </button>
        </div>
      ) : (
        <>
          {!imageLoaded && (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="animate-spin h-8 w-8 border-2 border-blue-500 border-t-transparent rounded-full" />
            </div>
          )}
          <img
            src={item.file_url}
            alt={item.filename}
            onLoad={handleImageLoad}
            onError={() => setImageError(true)}
            className={`select-none transition-opacity duration-200 ${
              imageLoaded ? 'opacity-100' : 'opacity-0'
            } ${fitMode ? 'max-w-full max-h-full object-contain' : ''}`}
            style={imgStyle}
            draggable={false}
          />
        </>
      )}
      {!fitMode && zoom !== 1 && (
        <div className="absolute bottom-4 right-4 bg-black/70 text-white text-xs px-2 py-1 rounded pointer-events-none">
          {Math.round(zoom * 100)}%
        </div>
      )}
    </div>
  );
}
