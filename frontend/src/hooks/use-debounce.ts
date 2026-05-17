import { useState, useEffect } from 'react';

/**
 * Debounce a value by `delay` milliseconds.
 *
 * Returns the debounced value, which updates only after the source value
 * has stopped changing for `delay` ms.
 */
export function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);

  return debounced;
}
