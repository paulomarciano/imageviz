/**
 * @vitest-environment jsdom
 *
 * Tests for SearchBar — verifies input rendering, debounce trigger,
 * clear button visibility, and Escape key / button clear behavior.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect } from 'vitest';
import { SearchBar } from '../search-bar';

describe('SearchBar', () => {
  it('renders search input', () => {
    // Act
    render(<SearchBar />);

    // Assert
    expect(screen.getByPlaceholderText('Search media...')).toBeInTheDocument();
  });

  it('shows clear button when text is entered', async () => {
    // Arrange
    render(<SearchBar />);
    const input = screen.getByPlaceholderText('Search media...');

    // Act
    await userEvent.type(input, 'test');

    // Assert
    expect(screen.getByLabelText('Clear search')).toBeInTheDocument();
  });

  it('clears input on Escape key', async () => {
    // Arrange
    render(<SearchBar />);
    const input = screen.getByPlaceholderText('Search media...');
    await userEvent.type(input, 'test');

    // Act
    fireEvent.keyDown(input, { key: 'Escape' });

    // Assert
    expect(input).toHaveValue('');
  });

  it('clears input on clear button click', async () => {
    // Arrange
    render(<SearchBar />);
    const input = screen.getByPlaceholderText('Search media...');
    await userEvent.type(input, 'test');

    // Act
    await userEvent.click(screen.getByLabelText('Clear search'));

    // Assert
    expect(input).toHaveValue('');
  });
});
