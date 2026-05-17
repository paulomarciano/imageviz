/**
 * @vitest-environment jsdom
 *
 * Tests for DetailView — verifies rendering, keyboard navigation, body
 * scroll lock, and navigation button visibility.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, fireEvent } from '@testing-library/react';
import { renderWithProviders } from '../../../test-utils/render-utils';
import { DetailView } from '../detail-view';
import type { MediaItem } from '../../../types/media';

/* ------------------------------------------------------------------ */
/*  Fixture                                                            */
/* ------------------------------------------------------------------ */

const mockItems: MediaItem[] = [
  {
    id: '1',
    filename: 'image.png',
    path: '2025/image.png',
    mime_type: 'image/png',
    thumbnail_url: '/api/v1/media/1/thumbnail',
    width: 896,
    height: 1216,
    file_size: 245_760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
  },
  {
    id: '2',
    filename: 'video.mp4',
    path: '2025/video.mp4',
    mime_type: 'video/mp4',
    thumbnail_url: '/api/v1/media/2/thumbnail',
    width: 1920,
    height: 1080,
    file_size: 10_485_760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
  },
];

const defaultProps = {
  items: mockItems,
  currentIndex: 0,
  onNavigate: vi.fn(),
  onClose: vi.fn(),
};

/* ------------------------------------------------------------------ */
/*  Tests                                                              */
/* ------------------------------------------------------------------ */

describe('DetailView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    document.body.style.overflow = '';
  });

  /* ---------- Rendering ---------- */

  it('renders nothing when items array is empty', () => {
    const { container } = renderWithProviders(
      <DetailView items={[]} currentIndex={0} onNavigate={vi.fn()} onClose={vi.fn()} />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('renders nothing when currentIndex is out of bounds', () => {
    const { container } = renderWithProviders(
      <DetailView items={mockItems} currentIndex={99} onNavigate={vi.fn()} onClose={vi.fn()} />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('renders the overlay container', () => {
    renderWithProviders(<DetailView {...defaultProps} />);
    const overlay = document.querySelector('.fixed.inset-0');
    expect(overlay).toBeInTheDocument();
  });

  /* ---------- Close button ---------- */

  it('shows close button', () => {
    renderWithProviders(<DetailView {...defaultProps} />);
    expect(screen.getByLabelText('Close detail view')).toBeInTheDocument();
  });

  it('calls onClose when close button clicked', () => {
    const onClose = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} onClose={onClose} />);
    fireEvent.click(screen.getByLabelText('Close detail view'));
    expect(onClose).toHaveBeenCalledOnce();
  });

  /* ---------- Navigation buttons ---------- */

  it('shows previous button when not at first item', () => {
    renderWithProviders(<DetailView {...defaultProps} currentIndex={1} />);
    expect(screen.getByLabelText('Previous item')).toBeInTheDocument();
  });

  it('does not show previous button at first item', () => {
    renderWithProviders(<DetailView {...defaultProps} currentIndex={0} />);
    expect(screen.queryByLabelText('Previous item')).not.toBeInTheDocument();
  });

  it('does not show next button at last item', () => {
    renderWithProviders(<DetailView {...defaultProps} currentIndex={1} />);
    expect(screen.queryByLabelText('Next item')).not.toBeInTheDocument();
  });

  it('shows next button when not at last item', () => {
    renderWithProviders(<DetailView {...defaultProps} currentIndex={0} />);
    expect(screen.getByLabelText('Next item')).toBeInTheDocument();
  });

  it('calls onNavigate with previous index when previous is clicked', () => {
    const onNavigate = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} currentIndex={1} onNavigate={onNavigate} />);
    fireEvent.click(screen.getByLabelText('Previous item'));
    expect(onNavigate).toHaveBeenCalledWith(0);
  });

  it('calls onNavigate with next index when next is clicked', () => {
    const onNavigate = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} currentIndex={0} onNavigate={onNavigate} />);
    fireEvent.click(screen.getByLabelText('Next item'));
    expect(onNavigate).toHaveBeenCalledWith(1);
  });

  /* ---------- Keyboard navigation ---------- */

  it('calls onClose on Escape key', () => {
    const onClose = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} onClose={onClose} />);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('navigates forward with ArrowRight', () => {
    const onNavigate = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} currentIndex={0} onNavigate={onNavigate} />);
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(onNavigate).toHaveBeenCalledWith(1);
  });

  it('navigates backward with ArrowLeft', () => {
    const onNavigate = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} currentIndex={1} onNavigate={onNavigate} />);
    fireEvent.keyDown(window, { key: 'ArrowLeft' });
    expect(onNavigate).toHaveBeenCalledWith(0);
  });

  it('does not navigate left when at first item', () => {
    const onNavigate = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} onNavigate={onNavigate} />);
    fireEvent.keyDown(window, { key: 'ArrowLeft' });
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it('does not navigate right when at last item', () => {
    const onNavigate = vi.fn();
    renderWithProviders(<DetailView {...defaultProps} currentIndex={1} onNavigate={onNavigate} />);
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(onNavigate).not.toHaveBeenCalled();
  });

  /* ---------- Body scroll lock ---------- */

  it('locks body scroll on mount', () => {
    renderWithProviders(<DetailView {...defaultProps} />);
    expect(document.body.style.overflow).toBe('hidden');
  });

  it('restores body scroll on unmount', () => {
    const { unmount } = renderWithProviders(<DetailView {...defaultProps} />);
    unmount();
    expect(document.body.style.overflow).toBe('');
  });

  /* ---------- Metadata panel ---------- */

  it('renders metadata panel', () => {
    renderWithProviders(<DetailView {...defaultProps} />);
    expect(screen.getByText('No metadata available for this file')).toBeInTheDocument();
  });

  /* ---------- Animation class ---------- */

  it('has fade-in animation class', () => {
    renderWithProviders(<DetailView {...defaultProps} />);
    const overlay = document.querySelector('.fixed.inset-0');
    expect(overlay).toHaveClass('animate-fade-in');
  });
});
