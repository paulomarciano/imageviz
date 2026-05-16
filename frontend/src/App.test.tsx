import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App', () => {
  it('renders without crashing', () => {
    // Arrange & Act
    render(<App />);

    // Assert
    expect(screen.getByText('ImageViz')).toBeInTheDocument();
  });
});
