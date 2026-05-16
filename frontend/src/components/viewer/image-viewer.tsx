import { useState, useCallback } from 'react';
import type { MediaItemDetail } from '../../types/media';

interface ImageViewerProps {
  readonly item: MediaItemDetail;
}

export function ImageViewer({ item }: ImageViewerProps) {
  const [zoom, setZoom] = useState(1);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [imageLoaded, setImageLoaded] = useState(false);
  const [imageError, setImageError] = useState(false);
  const [fitMode, setFitMode] = useState(true);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setZoom((prev) => Math.max(0.1, Math.min(10, prev * delta)));
    setFitMode(false);
  }, []);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (zoom > 1) {
        setIsDragging(true);
        setDragStart({ x: e.clientX - position.x, y: e.clientY - position.y });
      }
    },
    [zoom, position],
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (isDragging) {
        setPosition({
          x: e.clientX - dragStart.x,
          y: e.clientY - dragStart.y,
        });
      }
    },
    [isDragging, dragStart],
  );

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  const handleDoubleClick = useCallback(() => {
    if (fitMode) {
      setFitMode(false);
      setZoom(1);
    } else {
      setFitMode(true);
      setZoom(1);
      setPosition({ x: 0, y: 0 });
    }
  }, [fitMode]);

  const handleRetry = useCallback(() => {
    setImageError(false);
    setImageLoaded(false);
  }, []);

  return (
    <div
      className={`relative w-full h-full flex items-center justify-center bg-black/90 overflow-hidden select-none ${
        isDragging ? 'cursor-grabbing' : 'cursor-grab'
      }`}
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
          <button
            onClick={handleRetry}
            className="mt-2 text-blue-400 hover:text-blue-300"
          >
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
            onLoad={() => setImageLoaded(true)}
            onError={() => setImageError(true)}
            className={`select-none transition-opacity duration-200 ${
              imageLoaded ? 'opacity-100' : 'opacity-0'
            } ${fitMode ? 'max-w-full max-h-full object-contain' : ''}`}
            style={
              !fitMode
                ? {
                    transform: `translate(${position.x}px, ${position.y}px) scale(${zoom})`,
                    transformOrigin: 'center center',
                  }
                : undefined
            }
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
