/**
 * @vitest-environment jsdom
 *
 * Integration tests for the search flow — typing a query, seeing results,
 * clearing, and handling no-results states.
 *
 * Uses MSW to mock the backend API and renders the full App tree so we
 * exercise real component wiring through Jotai + TanStack Query.
 */

import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { handlers } from '../../test-utils/msw-handlers';
import { renderWithProviders } from '../../test-utils/render-utils';
import App from '../../App';

const server = setupServer(...handlers);

beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('Search Flow', () => {
  it('renders initial grid with media items', async () => {
    renderWithProviders(<App />);

    await waitFor(() => {
      expect(screen.getByText('image_mock-id-0.png')).toBeInTheDocument();
    });
  });

  it('shows search input and allows typing', async () => {
    renderWithProviders(<App />);

    const searchInput = screen.getByPlaceholderText('Search media...');
    expect(searchInput).toBeInTheDocument();

    await userEvent.type(searchInput, 'sunset');
    expect(searchInput).toHaveValue('sunset');
  });

  it('shows search results when search returns matches', async () => {
    renderWithProviders(<App />);

    const searchInput = screen.getByPlaceholderText('Search media...');
    await userEvent.type(searchInput, 'sunset');

    // Wait for debounce + search results
    await waitFor(
      () => {
        expect(screen.getAllByText(/result for "sunset"/).length).toBeGreaterThanOrEqual(1);
      },
      { timeout: 2000 },
    );
  });

  it('clears search and returns to full list', async () => {
    renderWithProviders(<App />);

    // First do a search
    const searchInput = screen.getByPlaceholderText('Search media...');
    await userEvent.type(searchInput, 'sunset');

    await waitFor(
      () => {
        expect(screen.getAllByText(/result for "sunset"/).length).toBeGreaterThanOrEqual(1);
      },
      { timeout: 2000 },
    );

    // Clear search
    await userEvent.click(screen.getByLabelText('Clear search'));

    // Wait for the list to return to browse mode items
    await waitFor(() => {
      expect(screen.getByText('image_mock-id-0.png')).toBeInTheDocument();
    });
  });

  it('shows no results message for non-matching search', async () => {
    // Override search handler to return empty results
    server.use(
      http.get('/api/v1/search', () => {
        return HttpResponse.json({
          data: [],
          meta: {
            next_cursor: null,
            next_cursor_id: null,
            has_more: false,
            total: 0,
            query: 'zzzznonexistent',
          },
        });
      }),
    );

    renderWithProviders(<App />);

    const searchInput = screen.getByPlaceholderText('Search media...');
    await userEvent.type(searchInput, 'zzzznonexistent');

    await waitFor(
      () => {
        expect(screen.getByText(/0 results/)).toBeInTheDocument();
      },
      { timeout: 2000 },
    );
  });
});
