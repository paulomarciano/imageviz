import { atom } from 'jotai';
import type { MediaItem } from '../types/media';

/** Which grid produced the current item list. */
export type MediaDataMode = 'browse' | 'search';

/** The derived flat list the active grid renders, plus its mode discriminator. */
export interface MediaDataState {
  readonly mode: MediaDataMode;
  readonly items: readonly MediaItem[];
}

/** Empty initial state — browse mode with no items. */
export const EMPTY_MEDIA_DATA: MediaDataState = { mode: 'browse', items: [] };

/**
 * Single source of truth for the active grid's flattened item list.
 *
 * Written by `ThumbnailGrid` (via effect on the flattened pages) and read by
 * `App.tsx` to drive `DetailView` navigation order. Holds serializable data
 * only — no TanStack Query internals.
 */
export const mediaDataAtom = atom<MediaDataState>(EMPTY_MEDIA_DATA);
