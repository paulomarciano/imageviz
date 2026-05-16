/**
 * @vitest-environment jsdom
 *
 * Tests for VideoViewer — verifies video element rendering, poster attribute,
 * loading state transitions, error handling with retry, and file info overlay.
 */

import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { VideoViewer } from '../video-viewer';
import { createMockMediaItem } from '../../../test-utils/render-utils';
import type { MediaItemDetail } from '../../../types/media';

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
});
