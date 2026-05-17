import { useEffect } from 'react';

const FOCUSABLE_SELECTOR =
  'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

/**
 * Trap Tab/Shift+Tab focus within a container element.
 *
 * When `isActive` is true, focus cycles among focusable elements inside
 * `containerRef`. The first focusable element receives focus on mount.
 *
 * @param containerRef - Ref to the container element
 * @param isActive - Whether the focus trap is active (default: true)
 */
export function useFocusTrap(containerRef: React.RefObject<HTMLElement | null>, isActive = true) {
  useEffect(() => {
    if (!isActive) return;
    const el = containerRef.current;
    if (!el) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      const focusable = el.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };

    el.addEventListener('keydown', handleKeyDown);
    const firstFocusable = el.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
    firstFocusable?.focus();
    return () => el.removeEventListener('keydown', handleKeyDown);
  }, [containerRef, isActive]);
}
