/**
 * @vitest-environment jsdom
 *
 * Tests for formatFileSize — boundary table across B → KB → MB → GB → TB.
 * Units are decimal (1000-based), matching the previous per-component copies.
 */

import { describe, it, expect } from 'vitest';
import { formatFileSize } from '../format';

describe('formatFileSize', () => {
  it.each([
    // [input bytes, expected output]
    [0, '0 B'],
    [999, '999 B'],
    [1_000, '1.0 KB'],
    [1_499, '1.5 KB'],
    [999_999, '1000.0 KB'],
    [1_000_000, '1.0 MB'],
    [1_500_000, '1.5 MB'],
    [999_999_999, '1000.0 MB'],
    [1_000_000_000, '1.0 GB'],
    [2_500_000_000, '2.5 GB'],
    [999_999_999_999, '1000.0 GB'],
    [1_000_000_000_000, '1.0 TB'],
    [2_750_000_000_000, '2.8 TB'],
  ])('formats %d bytes as %s', (bytes, expected) => {
    expect(formatFileSize(bytes)).toBe(expected);
  });
});
