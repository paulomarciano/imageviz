import { describe, it, expect } from 'vitest';
import { screen } from '@testing-library/react';
import { renderWithProviders } from './test-utils/render-utils';
import App from './App';

describe('App', () => {
  it('renders without crashing', () => {
    // Arrange & Act
    renderWithProviders(<App />);

    // Assert
    expect(screen.getByText('ImageViz')).toBeInTheDocument();
  });
});
