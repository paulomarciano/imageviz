/**
 * @vitest-environment jsdom
 *
 * Tests for ImageViewer — verifies image rendering, loading states, error
 * handling with retry, zoom indicator visibility, and drag cursor classes.
 */

import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ImageViewer } from '../image-viewer';
import { createMockMediaItem } from '../../../test-utils/render-utils';
import type { MediaItemDetail } from '../../../types/media';

/* ------------------------------------------------------------------ */
/*  Fixture                                                            */
/* ------------------------------------------------------------------ */

function createMockDetail(overrides?: Partial<MediaItemDetail>): MediaItemDetail {
  const base = createMockMediaItem(overrides);
  return {
    ...base,
    file_url: '/api/v1/media/test-id/file',
    metadata: null,
    ...overrides,
  };
}

/* ------------------------------------------------------------------ */
/*  Tests                                                              */
/* ------------------------------------------------------------------ */

describe('ImageViewer', () => {
  /* ---------- Rendering ---------- */

  it('renders image with file_url as src', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);
    const img = screen.getByRole('img');

    // Assert
    expect(img).toHaveAttribute('src', '/api/v1/media/test-id/file');
    expect(img).toHaveAttribute('alt', 'test.png');
  });

  it('shows loading spinner before image loads', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);

    // Assert
    expect(document.querySelector('.animate-spin')).toBeInTheDocument();
  });

  it('hides spinner and shows image after load', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);
    const img = screen.getByRole('img');
    fireEvent.load(img);

    // Assert
    expect(document.querySelector('.animate-spin')).not.toBeInTheDocument();
    expect(img).toHaveClass('opacity-100');
  });

  /* ---------- Error state ---------- */

  it('shows error state on image error', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);
    const img = screen.getByRole('img');
    fireEvent.error(img);

    // Assert
    expect(screen.getByText('Unable to load image')).toBeInTheDocument();
  });

  it('renders retry button in error state', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);
    fireEvent.error(screen.getByRole('img'));

    // Assert
    expect(screen.getByText('Retry')).toBeInTheDocument();
  });

  it('clears error and reloads image when retry is clicked', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);
    fireEvent.error(screen.getByRole('img'));

    const retryButton = screen.getByText('Retry');
    fireEvent.click(retryButton);

    // Assert — spinner should reappear, image should be hidden
    expect(document.querySelector('.animate-spin')).toBeInTheDocument();
    expect(screen.queryByText('Unable to load image')).not.toBeInTheDocument();
  });

  /* ---------- Zooming & panning ---------- */

  it('applies cursor-grab class to the container', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    const { container } = render(<ImageViewer item={item} />);

    // Assert
    const root = container.firstChild as HTMLElement;
    expect(root).toHaveClass('cursor-grab');
  });

  it('has cursor-grab class for pan interaction hint', () => {
    // Arrange
    const item = createMockDetail();

    // Act
    render(<ImageViewer item={item} />);

    // The container always shows cursor-grab; active:cursor-grabbing is
    // handled by CSS pseudo-class during mousedown (no React state change).
    const container = document.querySelector('[class*="cursor-grab"]')!;
    expect(container).toHaveClass('cursor-grab');
    expect(container).toHaveClass('active:cursor-grabbing');
  });

  /* ---------- Zoom indicator ---------- */

  it('shows zoom percentage indicator when zoomed in', async () => {
    // Arrange
    const item = createMockDetail();
    vi.useFakeTimers();

    // Act
    render(<ImageViewer item={item} />);
    const container = document.querySelector('[class*="cursor-grab"]')!;
    // Simulate wheel zoom (deltaY < 0 = zoom in, factor 1.1)
    fireEvent.wheel(container, { deltaY: -100 });

    // Zoom state is debounced at 50ms — flush the timer.
    await act(async () => {
      vi.advanceTimersByTime(60);
    });

    // Assert — zoom is 1.1 → shows "110%"
    expect(screen.getByText(/110/)).toBeInTheDocument();

    vi.useRealTimers();
  });
});
