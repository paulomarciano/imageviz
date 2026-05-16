import { atom } from 'jotai';
import { atomWithStorage, createJSONStorage } from 'jotai/utils';

/**
 * Saved grid scroll index for scroll restoration across navigation.
 * Uses sessionStorage (per-tab, cleared on tab close) so each tab
 * independently remembers its scroll position.
 */
export const gridScrollIndexAtom = atomWithStorage<number>(
  'imageviz-grid-index',
  0,
  createJSONStorage(() => sessionStorage),
);

/** Whether the keyboard shortcuts overlay is visible. */
export const shortcutsPanelOpenAtom = atom<boolean>(false);

/** Whether the configuration panel is open. */
export const configPanelOpenAtom = atom<boolean>(false);
