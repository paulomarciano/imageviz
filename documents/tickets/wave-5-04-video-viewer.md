# Wave 5.4 — Build Detail Viewer (Video Mode — Playback)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 04 |
| **Estimate** | 2.5 hours |
| **Depends on** | 4.1 (API types) |
| **Parallel** | Yes — can run in parallel with 5.3 (image viewer) |

---

## Overview

Build the video detail viewer that plays video files using the HTML5 `<video>` element. The video is served from the backend with Range support (Wave 2.7), enabling seeking and efficient playback without downloading the entire file.

## Prerequisites

- API types (4.1) — `MediaItemDetail` with `file_url`
- File serving with Range support (2.7) — backend must stream video with 206 Partial Content
- `file_url` points to `/api/v1/media/:id/file`

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 5 task 5.4, §8.2 streaming file serving, §9 Risk Register (Range requests for video)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/viewer/
├── video-viewer.tsx             # Video player component
└── __tests__/
    └── video-viewer.test.tsx    # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Displays video via `<video>` element with `src` set to `file_url`
- [ ] Browser-native video controls are shown (play/pause, seek bar, volume, fullscreen)
- [ ] Video loads and plays smoothly (browser handles Range requests automatically)
- [ ] Poster image shown before playback (use thumbnail_url as poster)
- [ ] Loading state while video metadata/buffering
- [ ] Error state if video fails to load ("Unable to play video")
- [ ] Supports MP4 and WEBM formats
- [ ] Keyboard: `Space` to play/pause, left/right arrows to seek ±5s, `F` to toggle fullscreen
- [ ] Background is dark/black

## Implementation Notes

```tsx
import { useState, useRef, useCallback, useEffect } from 'react';
import type { MediaItemDetail } from '../../types/media';

interface VideoViewerProps {
  item: MediaItemDetail;
}

export function VideoViewer({ item }: VideoViewerProps) {
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  const handleLoadedMetadata = useCallback(() => {
    setIsLoading(false);
  }, []);

  const handleError = useCallback(() => {
    setError('Unable to play video. The file may be corrupted or in an unsupported format.');
    setIsLoading(false);
  }, []);

  const handleRetry = useCallback(() => {
    setError(null);
    setIsLoading(true);
    // Reload the video
    if (videoRef.current) {
      videoRef.current.load();
    }
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const video = videoRef.current;
      if (!video) return;

      switch (e.key) {
        case ' ':
          e.preventDefault();
          video.paused ? video.play() : video.pause();
          break;
        case 'ArrowLeft':
          video.currentTime = Math.max(0, video.currentTime - 5);
          break;
        case 'ArrowRight':
          video.currentTime = Math.min(video.duration, video.currentTime + 5);
          break;
        case 'f':
        case 'F':
          if (document.fullscreenElement) {
            document.exitFullscreen();
          } else {
            video.requestFullscreen();
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Metadata display
  const formatDuration = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div className="w-full h-full flex items-center justify-center bg-black">
      {error ? (
        <div className="text-gray-400 text-center">
          <svg className="w-12 h-12 mx-auto mb-3 text-gray-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
              d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
          </svg>
          <p className="text-lg mb-2">{error}</p>
          <button onClick={handleRetry}
            className="text-blue-400 hover:text-blue-300">Retry</button>
        </div>
      ) : (
        <>
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
            preload="metadata"
            onLoadedMetadata={handleLoadedMetadata}
            onError={handleError}
            className="max-w-full max-h-full"
            // Note: TypeScript types for onLoadedMetadata might need casting
          >
            Your browser does not support the video tag.
          </video>
        </>
      )}

      {/* File info overlay */}
      {!error && !isLoading && (
        <div className="absolute top-4 left-4 bg-black/70 text-white text-xs px-2 py-1 rounded">
          {item.filename}
          {item.width && item.height && ` (${item.width}×${item.height})`}
        </div>
      )}
    </div>
  );
}
```

**Why use native `<video>` controls:**
- Comprehensive: play, pause, volume, seek, fullscreen, PiP
- Well-tested across browsers
- No additional dependencies
- Range request seeking works automatically if the server supports it (which it does via Wave 2.7)

**Poster image:** Using the thumbnail as a poster provides an instant preview before the video starts buffering. The thumbnail is small and loads quickly.

**Cross-browser video formats:**
- MP4 (H.264 + AAC) — supported by all modern browsers
- WEBM (VP8/VP9 + Vorbis/Opus) — supported by Chrome, Firefox, Edge
- The backend serves whatever format the file is in; the browser handles decoding

## Test Strategy

```tsx
import { render, screen } from '@testing-library/react';
import { VideoViewer } from '../video-viewer';

// jsdom doesn't fully support video elements (no media playback)
// Focus tests on structure and error handling
describe('VideoViewer', () => {
  it('renders video element with correct src', () => {
    render(<VideoViewer item={mockVideoItem} />);
    const video = screen.getByTitle(mockVideoItem.filename); // or use a role
    expect(video).toHaveAttribute('src', '/api/v1/media/video-1/file');
  });

  it('shows error state on video error', () => {
    render(<VideoViewer item={mockVideoItem} />);
    const video = screen.getByRole('video') || screen.getByTagName('video')[0];
    fireEvent.error(video);
    expect(screen.getByText(/Unable to play video/)).toBeInTheDocument();
  });

  it('uses thumbnail as poster', () => {
    render(<VideoViewer item={mockVideoItem} />);
    const video = screen.getByTagName('video')[0];
    expect(video).toHaveAttribute('poster', '/api/v1/media/video-1/thumbnail');
  });
});
```
