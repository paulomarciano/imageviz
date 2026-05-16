import { atom } from 'jotai';

export const searchQueryAtom = atom<string>('');

export type MediaTypeFilter = 'all' | 'image' | 'video';

export const mediaTypeFilterAtom = atom<MediaTypeFilter>('all');

export type SearchSort = 'recency' | 'score';

export const searchSortAtom = atom<SearchSort>('recency');

/** Returns the mime_type LIKE pattern for the current filter, or undefined for "all". */
export function mimeTypePattern(filter: MediaTypeFilter): string | undefined {
  switch (filter) {
    case 'image':
      return 'image/%';
    case 'video':
      return 'video/%';
    case 'all':
      return undefined;
  }
}

export const mediaViewModeAtom = atom<'browse' | 'search'>((get) => {
  const query = get(searchQueryAtom);
  return query.trim().length > 0 ? 'search' : 'browse';
});
