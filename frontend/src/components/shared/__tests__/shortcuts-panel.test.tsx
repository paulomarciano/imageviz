/**
 * @vitest-environment jsdom
 *
 * Tests for ShortcutsPanel — rendering, Escape-to-close via the shared
 * useEscape hook (gated on isOpen), and backdrop click-to-close.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ShortcutsPanel } from '../shortcuts-panel';

describe('ShortcutsPanel', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders nothing when closed', () => {
    const { container } = render(<ShortcutsPanel isOpen={false} onClose={vi.fn()} />);
    expect(container.innerHTML).toBe('');
  });

  it('renders the shortcuts dialog when open', () => {
    render(<ShortcutsPanel isOpen onClose={vi.fn()} />);
    expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeInTheDocument();
  });

  it('calls onClose once when Escape is pressed while open', () => {
    // Arrange
    const onClose = vi.fn();
    render(<ShortcutsPanel isOpen onClose={onClose} />);

    // Act
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('ignores Escape while closed (no listener attached)', () => {
    // Arrange
    const onClose = vi.fn();
    const { rerender } = render(<ShortcutsPanel isOpen={false} onClose={onClose} />);

    // Act — Escape while closed does nothing…
    fireEvent.keyDown(window, { key: 'Escape' });
    // …and after closing, a previously-open panel stops responding.
    rerender(<ShortcutsPanel isOpen onClose={onClose} />);
    rerender(<ShortcutsPanel isOpen={false} onClose={onClose} />);
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes on backdrop click but not on dialog click', () => {
    // Arrange
    const onClose = vi.fn();
    const { container } = render(<ShortcutsPanel isOpen onClose={onClose} />);

    // Act — click inside the dialog (stopPropagation)…
    fireEvent.click(screen.getByRole('dialog', { name: 'Keyboard shortcuts' }));
    expect(onClose).not.toHaveBeenCalled();

    // …then click the backdrop.
    const backdrop = container.firstElementChild as HTMLElement;
    fireEvent.click(backdrop);

    // Assert
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
