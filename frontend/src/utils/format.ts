/**
 * Shared formatting utilities for the ImageViz frontend.
 *
 * @module
 */

/** Format a byte count into a human-readable file-size string. */
export function formatFileSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}
