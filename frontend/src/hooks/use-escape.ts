import { useEffect } from 'react';

/**
 * Invoke `onClose` when the Escape key is pressed anywhere (window-level
 * `keydown`). Shared close handler for modal panels (config, shortcuts,
 * detail view).
 *
 * @param onClose - Callback fired on Escape.
 * @param enabled - When false, no listener is attached (e.g. a panel that
 *                  renders `null` while closed).
 */
export function useEscape(onClose: () => void, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, enabled]);
}
