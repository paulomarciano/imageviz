import { atom } from 'jotai';
import type { MediaItem } from '../types/media';

/** The currently selected media item for the detail view. */
export const selectedMediaItemAtom = atom<MediaItem | null>(null);

/** Whether the detail view is open. */
export const detailViewOpenAtom = atom<boolean>(false);
