import { useState, useCallback } from 'react';
import type { MediaMetadata } from '../../types/media';

interface MetadataPanelProps {
  readonly metadata: MediaMetadata | null;
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
    <div className="h-full overflow-y-auto p-3 bg-gray-800/70 border-l border-gray-700">
      <h3 className="text-sm font-medium text-gray-300 mb-3">Metadata</h3>

      {metadata.prompt && (
        <div className="mb-4">
          <JsonNode keyName="prompt" value={metadata.prompt} depth={0} defaultExpanded />
        </div>
      )}

      {metadata.workflow && (
        <div className="mb-4">
          <h4 className="text-xs font-medium text-gray-400 mb-1">Workflow</h4>
          <JsonNode
            keyName="workflow"
            value={metadata.workflow}
            depth={0}
            defaultExpanded={false}
          />
        </div>
      )}

      {!metadata.prompt && !metadata.workflow && (
        <p className="text-gray-500 text-sm">No structured metadata found.</p>
      )}
    </div>
  );
}

interface JsonNodeProps {
  readonly keyName: string;
  readonly value: unknown;
  readonly depth: number;
  readonly defaultExpanded?: boolean;
}

function ValueDisplay({ value }: { readonly value: unknown }) {
  if (typeof value === 'string') {
    return <span className="text-green-400">{`"${value}"`}</span>;
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

function JsonNode({ keyName, value, depth, defaultExpanded = false }: JsonNodeProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const isExpandable = typeof value === 'object' && value !== null;

  const toggle = useCallback(() => {
    if (isExpandable) setExpanded((prev) => !prev);
  }, [isExpandable]);

  const indent = depth * 16;

  if (Array.isArray(value)) {
    return (
      <div style={{ paddingLeft: indent }} className="font-mono text-xs py-0.5">
        <button
          onClick={toggle}
          className="text-gray-400 hover:text-white mr-1"
          aria-label={expanded ? 'Collapse' : 'Expand'}
        >
          {expanded ? '\u25BC' : '\u25B6'}
        </button>
        <span className="text-blue-300">{keyName}</span>
        <span className="text-gray-500">{`: Array[${value.length}]`}</span>
        {expanded &&
          value.map((item, i) => (
            <JsonNode key={i} keyName={String(i)} value={item} depth={depth + 1} />
          ))}
      </div>
    );
  }

  if (typeof value === 'object' && value !== null) {
    const entries = Object.entries(value as Record<string, unknown>);
    return (
      <div style={{ paddingLeft: indent }} className="font-mono text-xs py-0.5">
        <button
          onClick={toggle}
          className="text-gray-400 hover:text-white mr-1"
          aria-label={expanded ? 'Collapse' : 'Expand'}
        >
          {expanded ? '\u25BC' : '\u25B6'}
        </button>
        <span className="text-blue-300">{keyName}</span>
        <span className="text-gray-500">{`: {} ${entries.length} keys`}</span>
        {expanded &&
          entries.map(([k, v]) => <JsonNode key={k} keyName={k} value={v} depth={depth + 1} />)}
      </div>
    );
  }

  return (
    <div style={{ paddingLeft: indent }} className="font-mono text-xs py-0.5">
      <span className="text-blue-300">{keyName}</span>
      <span className="text-gray-500">: </span>
      <ValueDisplay value={value} />
    </div>
  );
}
