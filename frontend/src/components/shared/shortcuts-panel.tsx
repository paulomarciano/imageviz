import { useEffect, useRef } from 'react';

interface Shortcut {
  readonly keys: string[];
  readonly description: string;
}

const SHORTCUT_SECTIONS: { category: string; items: Shortcut[] }[] = [
  {
    category: 'Global',
    items: [
      { keys: ['?'], description: 'Show keyboard shortcuts' },
      { keys: ['/'], description: 'Focus search bar' },
      { keys: ['Esc'], description: 'Close panel / detail view' },
    ],
  },
  {
    category: 'Grid',
    items: [
      { keys: ['↑', '↓', '←', '→'], description: 'Navigate between items' },
      { keys: ['Enter'], description: 'Open detail view' },
      { keys: ['Space'], description: 'Select item' },
      { keys: ['Home'], description: 'Jump to first item' },
      { keys: ['End'], description: 'Jump to last item' },
    ],
  },
  {
    category: 'Detail View',
    items: [
      { keys: ['←', '→'], description: 'Previous / Next item' },
      { keys: ['Esc'], description: 'Close detail view' },
    ],
  },
  {
    category: 'Image Viewer',
    items: [
      { keys: ['Scroll'], description: 'Zoom in / out' },
      { keys: ['Double-click'], description: 'Toggle fit / 100%' },
      { keys: ['Click + Drag'], description: 'Pan when zoomed' },
      { keys: ['+', '-'], description: 'Zoom in / out (keyboard)' },
    ],
  },
  {
    category: 'Video Viewer',
    items: [
      { keys: ['Space'], description: 'Play / Pause' },
      { keys: ['←', '→'], description: 'Seek backward / forward 5s' },
      { keys: ['F'], description: 'Toggle fullscreen' },
    ],
  },
];

interface ShortcutsPanelProps {
  readonly isOpen: boolean;
  readonly onClose: () => void;
}

export function ShortcutsPanel({ isOpen, onClose }: ShortcutsPanelProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  // Close on Escape
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  // Trap focus inside the panel modal
  useEffect(() => {
    if (!isOpen || !panelRef.current) return;

    const element = panelRef.current;
    const focusableSelector =
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

    const handleTabKey = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;

      const focusableElements =
        element.querySelectorAll<HTMLElement>(focusableSelector);
      if (focusableElements.length === 0) return;

      const first = focusableElements[0]!;
      const last = focusableElements[focusableElements.length - 1]!;

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };

    element.addEventListener('keydown', handleTabKey);

    // Focus the first focusable element on open
    const firstFocusable =
      element.querySelector<HTMLElement>(focusableSelector);
    firstFocusable?.focus();

    return () => element.removeEventListener('keydown', handleTabKey);
  }, [isOpen]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center"
      onClick={onClose}
    >
      <div
        ref={panelRef}
        className="bg-gray-900 border border-gray-700 rounded-lg shadow-2xl max-w-lg w-full mx-4 max-h-[80vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Keyboard shortcuts"
        aria-modal="true"
      >
        <div className="flex items-center justify-between p-4 border-b border-gray-700">
          <h2 className="text-lg font-semibold text-white">
            Keyboard Shortcuts
          </h2>
          <button
            onClick={onClose}
            className="p-1 text-gray-400 hover:text-white transition-colors rounded"
            aria-label="Close shortcuts"
          >
            <svg
              className="w-5 h-5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>

        <div className="p-4 space-y-6">
          {SHORTCUT_SECTIONS.map(({ category, items }) => (
            <section key={category}>
              <h3 className="text-sm font-medium text-gray-300 mb-2">
                {category}
              </h3>
              <div className="space-y-1.5">
                {items.map(({ keys, description }) => (
                  <div
                    key={description}
                    className="flex items-center justify-between text-sm"
                  >
                    <span className="text-gray-400">{description}</span>
                    <div className="flex gap-1 ml-4">
                      {keys.map((key) => (
                        <kbd
                          key={key}
                          className="px-2 py-0.5 bg-gray-800 border border-gray-600 rounded text-xs text-gray-200 font-mono"
                        >
                          {key}
                        </kbd>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>

        <div className="p-3 border-t border-gray-700 text-center">
          <p className="text-xs text-gray-500">
            Press{' '}
            <kbd className="px-1 py-0.5 bg-gray-800 border border-gray-600 rounded text-xs font-mono">
              Esc
            </kbd>{' '}
            to close
          </p>
        </div>
      </div>
    </div>
  );
}
