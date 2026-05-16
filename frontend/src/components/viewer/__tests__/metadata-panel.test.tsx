/**
 * @vitest-environment jsdom
 *
 * Tests for MetadataPanel — verifies rendering, expand/collapse behavior,
 * syntax highlighting, and edge cases (null metadata, empty prompt/workflow).
 */

import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MetadataPanel } from '../metadata-panel';

describe('MetadataPanel', () => {
  it('shows no metadata message when null', () => {
    render(<MetadataPanel metadata={null} />);
    expect(
      screen.getByText('No metadata available for this file'),
    ).toBeInTheDocument();
  });

  it('renders prompt data as JSON tree', () => {
    const metadata = {
      prompt: { seed: 12345, model: 'SDXL' },
      workflow: null,
    };
    render(<MetadataPanel metadata={metadata} />);
    expect(screen.getByText('prompt')).toBeInTheDocument();
  });

  it('renders workflow section', () => {
    const metadata = {
      prompt: null,
      workflow: { nodes: [{ id: 1, type: 'KSampler' }] },
    };
    render(<MetadataPanel metadata={metadata} />);
    expect(screen.getByText('workflow')).toBeInTheDocument();
  });

  it('shows no structured metadata when both are null', () => {
    const metadata = { prompt: null, workflow: null };
    render(<MetadataPanel metadata={metadata} />);
    expect(
      screen.getByText('No structured metadata found.'),
    ).toBeInTheDocument();
  });

  it('expands prompt by default and shows children', () => {
    const metadata = {
      prompt: { seed: 12345, model: 'SDXL' },
      workflow: null,
    };
    render(<MetadataPanel metadata={metadata} />);
    // prompt is expanded by default, so children should be visible
    expect(screen.getByText('seed')).toBeInTheDocument();
    expect(screen.getByText('model')).toBeInTheDocument();
  });

  it('collapses workflow by default', () => {
    const metadata = {
      prompt: null,
      workflow: { nested: { deep: 'value' } },
    };
    render(<MetadataPanel metadata={metadata} />);
    // workflow is collapsed — children (nested, deep) should NOT be visible
    expect(screen.getByText('workflow')).toBeInTheDocument();
    expect(screen.queryByText('nested')).not.toBeInTheDocument();
    expect(screen.queryByText('deep')).not.toBeInTheDocument();
  });

  it('toggles expand/collapse on click', () => {
    const metadata = {
      prompt: { nested: { deep: 'value' } },
      workflow: null,
    };
    render(<MetadataPanel metadata={metadata} />);

    // Toggle button for the nested object (should be collapsed by default)
    const toggleButtons = screen.getAllByRole('button', { name: 'Expand' });
    // First click should expand
    fireEvent.click(toggleButtons[0]!);
    expect(screen.getByText('deep')).toBeInTheDocument();

    // Now buttons should be 'Collapse'
    const collapseButtons = screen.getAllByRole('button', {
      name: 'Collapse',
    });
    fireEvent.click(collapseButtons[0]!);
    expect(screen.queryByText('deep')).not.toBeInTheDocument();
  });

  it('renders string values in green', () => {
    const metadata = { prompt: { model: 'SDXL' }, workflow: null };
    render(<MetadataPanel metadata={metadata} />);
    const valueSpan = screen.getByText('"SDXL"');
    expect(valueSpan).toHaveClass('text-green-400');
  });

  it('renders number values in yellow', () => {
    const metadata = { prompt: { seed: 12345 }, workflow: null };
    render(<MetadataPanel metadata={metadata} />);
    const valueSpan = screen.getByText('12345');
    expect(valueSpan).toHaveClass('text-yellow-400');
  });

  it('renders boolean values in purple', () => {
    const metadata = { prompt: { enabled: true }, workflow: null };
    render(<MetadataPanel metadata={metadata} />);
    const valueSpan = screen.getByText('true');
    expect(valueSpan).toHaveClass('text-purple-400');
  });

  it('renders null values in gray', () => {
    const metadata = { prompt: { value: null }, workflow: null };
    render(<MetadataPanel metadata={metadata} />);
    const valueSpan = screen.getByText('null');
    expect(valueSpan).toHaveClass('text-gray-500');
  });

  it('renders arrays with bracket notation', () => {
    const metadata = {
      prompt: { tags: ['fast', 'quality'] },
      workflow: null,
    };
    render(<MetadataPanel metadata={metadata} />);
    expect(screen.getByText(/Array\[2\]/)).toBeInTheDocument();
  });

  it('renders objects with key count', () => {
    const metadata = {
      prompt: { settings: { a: 1, b: 2, c: 3 } },
      workflow: null,
    };
    render(<MetadataPanel metadata={metadata} />);
    expect(screen.getByText(/{} 3 keys/)).toBeInTheDocument();
  });

  it('has independent scroll via overflow-y-auto', () => {
    const metadata = {
      prompt: { seed: 12345 },
      workflow: { nodes: [] },
    };
    const { container } = render(<MetadataPanel metadata={metadata} />);
    const panel = container.firstChild as HTMLElement;
    expect(panel.className).toContain('overflow-y-auto');
  });
});
