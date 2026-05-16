import { atom } from 'jotai';

export const searchQueryAtom = atom<string>('');

export const mediaViewModeAtom = atom<'browse' | 'search'>((get) => {
  const query = get(searchQueryAtom);
  return query.trim().length > 0 ? 'search' : 'browse';
});
