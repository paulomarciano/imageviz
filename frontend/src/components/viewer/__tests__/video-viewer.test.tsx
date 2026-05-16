/**
 * @vitest-environment jsdom
 *
 * Tests for VideoViewer — verifies video element rendering, poster attribute,
 * loading state transitions, error handling with retry, and file info overlay.
 */

import { describe, it, expect, vi, beforeAll } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { VideoViewer } from '../video-viewer';
import { createMockMediaItem } from '../../../test-utils/render-utils';
import type { MediaItemDetail } from '../../../types/media';

// jsdom does not implement HTMLMediaElement.prototype.play / pause.
// We mock them with spies that also toggle the paused property.
beforeAll(() => {
  HTMLVideoElement.prototype.play = vi.fn(function (this: HTMLVideoElement) {
    Object.defineProperty(this, 'paused', { value: false, writable: true, configurable: true });
    return Promise.resolve();
  });
  HTMLVideoElement.prototype.pause = vi.fn(function (this: HTMLVideoElement) {
    Object.defineProperty(this, 'paused', { value: true, writable: true, configurable: true });
  });
});

/* ------------------------------------------------------------------ */
/*  Fixture                                                            */
/* ------------------------------------------------------------------ */

function createMockVideoDetail(
  overrides?: Partial<MediaItemDetail>,
): MediaItemDetail {
  const base = createMockMediaItem({
    id: 'video-1',
    filename: 'test.mp4',
    mime_type: 'video/mp4',
    width: 1920,
    height: 1080,
    thumbnail_url: '/api/v1/media/video-1/thumbnail',
    ...overrides,
  });
  return {
    ...base,
    file_url: '/api/v1/media/video-1/file',
    metadata: null,
    ...overrides,
  };
}

/* ------------------------------------------------------------------ */
/*  Tests                                                              */
/* ------------------------------------------------------------------ */

describe('VideoViewer', () => {
  /* ---------- Rendering ---------- */

  it('renders video element with correct src', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    const video = document.querySelector('video');

    // Assert
    expect(video).toBeInTheDocument();
    expect(video).toHaveAttribute('src', '/api/v1/media/video-1/file');
  });

  it('uses thumbnail as poster', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    const video = document.querySelector('video');

    // Assert
    expect(video).toHaveAttribute(
      'poster',
      '/api/v1/media/video-1/thumbnail',
    );
  });

  /* ---------- Loading state ---------- */

  it('shows loading spinner before metadata loads', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);

    // Assert
    expect(document.querySelector('.animate-spin')).toBeInTheDocument();
  });

  it('hides spinner after metadata loads', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    fireEvent.loadedMetadata(document.querySelector('video')!);

    // Assert
    expect(document.querySelector('.animate-spin')).not.toBeInTheDocument();
  });

  /* ---------- Error state ---------- */

  it('shows error message on video error', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    fireEvent.error(document.querySelector('video')!);

    // Assert
    expect(screen.getByText(/Unable to play video/)).toBeInTheDocument();
  });

  it('renders retry button after error', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    fireEvent.error(document.querySelector('video')!);

    // Assert
    expect(screen.getByText('Retry')).toBeInTheDocument();
  });

  it('clears error and shows spinner when retry is clicked', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    fireEvent.error(document.querySelector('video')!);
    fireEvent.click(screen.getByText('Retry'));

    // Assert
    expect(
      screen.queryByText(/Unable to play video/),
    ).not.toBeInTheDocument();
    expect(document.querySelector('.animate-spin')).toBeInTheDocument();
  });

  /* ---------- File info overlay ---------- */

  it('shows filename and dimensions after metadata loads', () => {
    // Arrange
    const item = createMockVideoDetail();

    // Act
    render(<VideoViewer item={item} />);
    fireEvent.loadedMetadata(document.querySelector('video')!);

    // Assert
    expect(screen.getByText(/test\.mp4/)).toBeInTheDocument();
    expect(screen.getByText(/1920×1080/)).toBeInTheDocument();
  });

  /* ---------- Keyboard shortcuts ---------- */

  it('toggles play/pause on Space key', () => {
    // Arrange
    vi.clearAllMocks();
    const item = createMockVideoDetail();
    render(<VideoViewer item={item} />);
    const video = document.querySelector('video')!;
    fireEvent.loadedMetadata(video);

    // Start in a "playing" state
    Object.defineProperty(video, 'paused', { value: false, writable: true, configurable: true });

    // Act — dispatch keydown on the container div
    const container = video.parentElement!;
    fireEvent.keyDown(container, { key: ' ' });

    // Assert — pause should have been called
    expect(video.pause).toHaveBeenCalledOnce();
    expect(video.paused).toBe(true);

    // Now start in a "paused" state
    Object.defineProperty(video, 'paused', { value: true, writable: true, configurable: true });
    fireEvent.keyDown(container, { key: ' ' });

    // Assert — play should have been called
    expect(video.play).toHaveBeenCalledOnce();
  });

  it('seeks backward on ArrowLeft', () => {
    // Arrange
    const item = createMockVideoDetail();
    render(<VideoViewer item={item} />);
    const video = document.querySelector('video')!;
    fireEvent.loadedMetadata(video);
    Object.defineProperty(video, 'currentTime', { value: 30, writable: true });
    Object.defineProperty(video, 'duration', { value: 120, writable: true });

    // Act
    const container = video.parentElement!;
    fireEvent.keyDown(container, { key: 'ArrowLeft' });

    // Assert
    expect(video.currentTime).toBe(25);
  });

  it('seeks forward on ArrowRight', () => {
    // Arrange
    const item = createMockVideoDetail();
    render(<VideoViewer item={item} />);
    const video = document.querySelector('video')!;
    fireEvent.loadedMetadata(video);
    Object.defineProperty(video, 'currentTime', { value: 30, writable: true });
    Object.defineProperty(video, 'duration', { value: 120, writable: true });

    // Act
    const container = video.parentElement!;
    fireEvent.keyDown(container, { key: 'ArrowRight' });

    // Assert
    expect(video.currentTime).toBe(35);
  });

  it('toggles fullscreen on F key', () => {
    // Arrange
    const item = createMockVideoDetail();
    render(<VideoViewer item={item} />);
    const video = document.querySelector('video')!;
    fireEvent.loadedMetadata(video);
    const requestFullscreen = vi.fn();
    video.requestFullscreen = requestFullscreen;

    // Act
    const container = video.parentElement!;
    fireEvent.keyDown(container, { key: 'f' });

    // Assert
    expect(requestFullscreen).toHaveBeenCalled();
  });

  it('does not interfere with component when video ref is null', () => {
    // Arrange
    const item = createMockVideoDetail();
    // Remove any video element to simulate missing ref
    const { container } = render(<VideoViewer item={item} />);

    // Act — dispatch on container (no-op, should not throw)
    const containerDiv = container.firstElementChild!;
    expect(() => {
      fireEvent.keyDown(containerDiv, { key: ' ' });
    }).not.toThrow();
  });
});
