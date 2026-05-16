/**
 * @vitest-environment jsdom
 *
 * Integration tests for the detail view flow — opening, closing, keyboard
 * dismissal, and metadata display.
 *
 * Uses MSW to mock the backend API and renders the full App tree so we
 * exercise real component wiring through Jotai + TanStack Query.
 */

import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { handlers } from '../../test-utils/msw-handlers';
import { renderWithProviders } from '../../test-utils/render-utils';
import App from '../../App';

const server = setupServer(...handlers);

beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('Detail View Flow', () => {
  it('opens detail view on thumbnail click', async () => {
    renderWithProviders(<App />);

    // Wait for grid items
    await waitFor(() => {
      expect(screen.getByText('image_mock-id-0.png')).toBeInTheDocument();
    });

    // Click the first thumbnail card (use aria-label to distinguish from settings button)
    const cards = screen.getAllByRole('button', { name: /^View / });
    await userEvent.click(cards[0]!);

    // Detail view should appear with close button
    await waitFor(() => {
      expect(screen.getByLabelText('Close detail view')).toBeInTheDocument();
    });
  });

  it('closes detail view when close button is clicked', async () => {
    renderWithProviders(<App />);

    await waitFor(() => {
      expect(screen.getByText('image_mock-id-0.png')).toBeInTheDocument();
    });

    const cards = screen.getAllByRole('button', { name: /^View / });
    await userEvent.click(cards[0]!);

    await waitFor(() => {
      expect(screen.getByLabelText('Close detail view')).toBeInTheDocument();
    });

    await userEvent.click(screen.getByLabelText('Close detail view'));

    await waitFor(() => {
      expect(screen.queryByLabelText('Close detail view')).not.toBeInTheDocument();
    });
  });

  it('closes detail view on Escape key', async () => {
    renderWithProviders(<App />);

    await waitFor(() => {
      expect(screen.getByText('image_mock-id-0.png')).toBeInTheDocument();
    });

    const cards = screen.getAllByRole('button', { name: /^View / });
    await userEvent.click(cards[0]!);

    await waitFor(() => {
      expect(screen.getByLabelText('Close detail view')).toBeInTheDocument();
    });

    await userEvent.keyboard('{Escape}');

    await waitFor(() => {
      expect(screen.queryByLabelText('Close detail view')).not.toBeInTheDocument();
    });
  });

  it('shows metadata in detail view', async () => {
    renderWithProviders(<App />);

    await waitFor(() => {
      expect(screen.getByText('image_mock-id-0.png')).toBeInTheDocument();
    });

    const cards = screen.getAllByRole('button', { name: /^View / });
    await userEvent.click(cards[0]!);

    // The metadata panel should render the 'prompt' key name
    await waitFor(() => {
      expect(screen.getByText('prompt')).toBeInTheDocument();
    });
  });
});
