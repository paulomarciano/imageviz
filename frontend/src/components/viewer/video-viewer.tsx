/**
 * Video viewer component with native controls, keyboard shortcuts, and
 * loading/error states. Uses the `<video>` element with browser-native
 * controls for play/pause, seek, volume, and fullscreen.
 *
 * Keyboard shortcuts are scoped to the container div (not window) so they
 * don't interfere with other UI elements like the metadata panel.
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import type { MediaItemDetail } from '../../types/media';

interface VideoViewerProps {
  readonly item: MediaItemDetail;
}

export function VideoViewer({ item }: VideoViewerProps) {
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  const handleLoadedMetadata = useCallback(() => {
    setIsLoading(false);
  }, []);

  // Explicitly attempt playback when the video element mounts.
  // The `autoPlay` attribute alone is unreliable for elements rendered
  // conditionally (e.g., inside a modal that may not be in the initial
  // DOM).  Calling `play()` in an effect ensures the browser receives the
  // play signal even when autoplay policy blocks the declarative attribute.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    // Browsers return a promise from play() that rejects if autoplay is
    // blocked (e.g. no user gesture, or the video has audio and is not
    // muted).  We catch the rejection silently since the user can press
    // the native play button.
    video.play().catch(() => {
      /* autoplay blocked — user can press play manually */
    });
  }, []);

  const handleError = useCallback(() => {
    setError('Unable to play video. The file may be corrupted or in an unsupported format.');
    setIsLoading(false);
  }, []);

  const handleRetry = useCallback(() => {
    setError(null);
    setIsLoading(true);
    videoRef.current?.load();
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    const video = videoRef.current;
    if (!video) return;

    switch (e.key) {
      case ' ':
        e.preventDefault();
        if (video.paused) {
          void video.play();
        } else {
          video.pause();
        }
        break;
      case 'ArrowLeft':
        e.preventDefault();
        video.currentTime = Math.max(0, video.currentTime - 5);
        break;
      case 'ArrowRight':
        e.preventDefault();
        video.currentTime = Math.min(video.duration, video.currentTime + 5);
        break;
      case 'f':
      case 'F':
        e.preventDefault();
        if (document.fullscreenElement) {
          void document.exitFullscreen();
        } else {
          void video.requestFullscreen();
        }
        break;
    }
  }, []);

  return (
    <div className="relative w-full h-full flex items-center justify-center bg-black">
      {error ? (
        <div className="text-gray-400 text-center">
          <svg
            className="w-12 h-12 mx-auto mb-3 text-gray-500"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.5}
              d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
            />
          </svg>
          <p className="text-lg mb-2">{error}</p>
          <button onClick={handleRetry} className="text-blue-400 hover:text-blue-300">
            Retry
          </button>
        </div>
      ) : (
        <div
          className="relative w-full h-full flex items-center justify-center"
          onKeyDown={handleKeyDown}
        >
          {isLoading && (
            <div className="absolute inset-0 flex items-center justify-center z-10">
              <div className="animate-spin h-8 w-8 border-2 border-blue-500 border-t-transparent rounded-full" />
            </div>
          )}
          <video
            ref={videoRef}
            src={item.file_url}
            poster={item.thumbnail_url}
            controls
            autoPlay
            muted
            preload="auto"
            onLoadedMetadata={handleLoadedMetadata}
            onError={handleError}
            className="max-w-full max-h-full"
          >
            Your browser does not support the video tag.
          </video>
        </div>
      )}
      {!error && !isLoading && (
        <div className="absolute top-4 left-4 bg-black/70 text-white text-xs px-2 py-1 rounded pointer-events-none">
          {item.filename}
          {item.width != null && item.height != null && ` (${item.width}\u00D7${item.height})`}
        </div>
      )}
    </div>
  );
}
