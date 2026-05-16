# Wave 5.5 — Build Metadata Panel (JSON Tree View)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 05 |
| **Estimate** | 2 hours |
| **Depends on** | 4.1 (API types), 5.3/5.4 (viewers — for layout context) |
| **Parallel** | Can run in parallel with 5.6 |

---

## Overview

Build a collapsible JSON tree view for displaying image metadata (ComfyUI prompt + workflow) in the detail view side panel. The panel shows parsed metadata with syntax highlighting and collapsible nodes for nested JSON structures.

## Prerequisites

- API types (4.1) — `MediaItemDetail.metadata`
- Detail view shell (5.6) — the panel lives alongside the viewer

## Reference Files

- `documents/plans/development-plan.md` — §10.Q9 (collapsible JSON tree with syntax highlighting), §3.3 MediaItem (detail) metadata field
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/viewer/
├── metadata-panel.tsx           # JSON tree view component
└── __tests__/
    └── metadata-panel.test.tsx  # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Renders metadata as a collapsible tree (expand/collapse nested objects and arrays)
- [ ] Syntax highlighting: keys in one color, string values in another, numbers in another, booleans in another
- [ ] All nodes collapsed by default (top-level only) — the prompt/workflow can be very large
- [ ] Click to expand/collapse nodes
- [ ] Long string values are truncated with "Show more" toggle
- [ ] Copy button on leaf values (click to copy to clipboard)
- [ ] Search/filter within metadata (optional but nice — filter keys/values)
- [ ] Shows "No metadata available" when metadata is null/empty
- [ ] Scrolls independently from the viewer
- [ ] Styled for dark theme with JSON-like color scheme

## Implementation Notes

**Build a custom JSON tree (no external library needed):**
```tsx
import { useState, useCallback } from 'react';

interface JsonNodeProps {
  keyName: string;
  value: unknown;
  depth: number;
  defaultExpanded?: boolean;
}

function JsonNode({ keyName, value, depth, defaultExpanded = false }: JsonNodeProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const isExpandable = typeof value === 'object' && value !== null;

  const toggle = useCallback(() => {
    if (isExpandable) setExpanded((prev) => !prev);
  }, [isExpandable]);

  const indent = depth * 16; // 16px per level

  if (Array.isArray(value)) {
    return (
      <div style={{ paddingLeft: indent }}>
        <button onClick={toggle} className="text-gray-400 hover:text-white mr-1 font-mono text-xs">
          {expanded ? '▼' : '▶'}
        </button>
        <span className="text-blue-300">{keyName}</span>
        <span className="text-gray-500">: Array[{value.length}]</span>
        {expanded && value.map((item, i) => (
          <JsonNode key={i} keyName={String(i)} value={item} depth={depth + 1} />
        ))}
      </div>
    );
  }

  if (typeof value === 'object' && value !== null) {
    const entries = Object.entries(value as Record<string, unknown>);
    return (
      <div style={{ paddingLeft: indent }}>
        <button onClick={toggle} className="text-gray-400 hover:text-white mr-1 font-mono text-xs">
          {expanded ? '▼' : '▶'}
        </button>
        <span className="text-blue-300">{keyName}</span>
        <span className="text-gray-500">: {'{}'} {entries.length} keys</span>
        {expanded && entries.map(([k, v]) => (
          <JsonNode key={k} keyName={k} value={v} depth={depth + 1} />
        ))}
      </div>
    );
  }

  // Leaf value
  return (
    <div style={{ paddingLeft: indent }} className="font-mono text-xs py-0.5">
      <span className="text-blue-300">{keyName}</span>
      <span className="text-gray-500">: </span>
      <ValueDisplay value={value} />
      <CopyButton value={value} />
    </div>
  );
}

function ValueDisplay({ value }: { value: unknown }) {
  if (typeof value === 'string') {
    return <span className="text-green-400">"{value}"</span>;
  }
  if (typeof value === 'number') {
    return <span className="text-yellow-400">{value}</span>;
  }
  if (typeof value === 'boolean') {
    return <span className="text-purple-400">{String(value)}</span>;
  }
  if (value === null) {
    return <span className="text-gray-500">null</span>;
  }
  return <span className="text-gray-300">{String(value)}</span>;
}

function CopyButton({ value }: { value: unknown }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    await navigator.clipboard.writeText(String(value));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [value]);

  return (
    <button
      onClick={handleCopy}
      className="ml-1 opacity-0 group-hover:opacity-100 hover:opacity-100 text-gray-500 hover:text-white text-xs"
      title="Copy to clipboard"
    >
      {copied ? '✓' : '📋'}
    </button>
  );
}
```

**Metadata panel container:**
```tsx
import type { MediaMetadata } from '../../types/media';

interface MetadataPanelProps {
  metadata: MediaMetadata | null;
}

export function MetadataPanel({ metadata }: MetadataPanelProps) {
  if (!metadata) {
    return (
      <div className="p-4 text-gray-500 text-sm text-center">
        No metadata available for this file
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-3 bg-gray-850 border-l border-gray-700">
      <h3 className="text-sm font-medium text-gray-300 mb-3">Metadata</h3>

      {metadata.prompt && (
        <div className="mb-4">
          <JsonNode keyName="prompt" value={metadata.prompt} depth={0} defaultExpanded />
        </div>
      )}

      {metadata.workflow && (
        <div className="mb-4">
          <h4 className="text-xs font-medium text-gray-400 mb-1">Workflow</h4>
          <JsonNode keyName="workflow" value={metadata.workflow} depth={0} defaultExpanded={false} />
        </div>
      )}

      {!metadata.prompt && !metadata.workflow && (
        <p className="text-gray-500 text-sm">No structured metadata found.</p>
      )}
    </div>
  );
}
```

## Test Strategy

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { MetadataPanel } from '../metadata-panel';

describe('MetadataPanel', () => {
  it('shows "no metadata" when null', () => {
    render(<MetadataPanel metadata={null} />);
    expect(screen.getByText('No metadata available for this file')).toBeInTheDocument();
  });

  it('renders prompt data as JSON tree', () => {
    const metadata = {
      prompt: { seed: 12345, model: 'SDXL' },
      workflow: null,
    };
    render(<MetadataPanel metadata={metadata} />);
    expect(screen.getByText('prompt')).toBeInTheDocument();
  });

  it('expands/collapses nodes on click', () => {
    const metadata = { prompt: { nested: { deep: 'value' } }, workflow: null };
    render(<MetadataPanel metadata={metadata} />);
    
    const toggle = screen.getAllByText('▶')[0];
    fireEvent.click(toggle);
    // Assert nested content is visible
  });
});
```
