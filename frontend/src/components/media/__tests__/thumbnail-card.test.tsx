/**
 * @vitest-environment jsdom
 *
 * Tests for ThumbnailCard — verifies rendering, interactions, and edge cases
 * including image loading failures and keyboard accessibility.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThumbnailCard } from '../thumbnail-card';
import { createMockMediaItem } from '../../../test-utils/render-utils';
import type { MediaItem } from '../../../types/media';

describe('ThumbnailCard', () => {
  const mockItem: MediaItem = createMockMediaItem();

  // ---------- Arrange ----------
  // mockItem is set up with deterministic defaults for all tests.

  it('renders filename', () => {
    // Act
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    // Assert
    expect(screen.getByText('test.png')).toBeInTheDocument();
  });

  it('renders dimensions and formatted file size', () => {
    // Act
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    // Assert
    expect(screen.getByText(/896×1216/)).toBeInTheDocument();
    // 245760 bytes = 245.8 KB (rounded to 1 decimal)
    expect(screen.getByText(/245.8 KB/)).toBeInTheDocument();
  });

  it('calls onClick when clicked', () => {
    // Arrange
    const onClick = vi.fn();

    // Act
    render(<ThumbnailCard item={mockItem} onClick={onClick} />);
    fireEvent.click(screen.getByRole('button'));

    // Assert
    expect(onClick).toHaveBeenCalledWith(mockItem);
  });

  it('calls onClick on Enter key', () => {
    // Arrange
    const onClick = vi.fn();

    // Act
    render(<ThumbnailCard item={mockItem} onClick={onClick} />);
    fireEvent.keyDown(screen.getByRole('button'), { key: 'Enter' });

    // Assert
    expect(onClick).toHaveBeenCalledWith(mockItem);
  });

  it('renders image with loading="lazy"', () => {
    // Act
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    const img = screen.getByRole('img');

    // Assert
    expect(img).toHaveAttribute('loading', 'lazy');
    expect(img).toHaveAttribute('src', mockItem.thumbnail_url);
  });

  it('shows placeholder when image fails to load', async () => {
    // Arrange
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    const img = screen.getByRole('img');

    // Act
    fireEvent.error(img);

    // Assert
    expect(await screen.findByText('No preview')).toBeInTheDocument();
  });

  it('has accessible button role and aria-label', () => {
    // Act
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    const button = screen.getByRole('button');

    // Assert
    expect(button).toHaveAttribute('aria-label', 'View test.png');
    expect(button).toHaveAttribute('tabIndex', '0');
  });
});
