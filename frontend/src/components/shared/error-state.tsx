import { useState } from 'react';

interface ErrorStateProps {
  readonly message: string;
  readonly details?: string;
  readonly onRetry?: () => void;
}

export function ErrorState({ message, details, onRetry }: ErrorStateProps) {
  const [showDetails, setShowDetails] = useState(false);

  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <svg
        className="w-12 h-12 text-red-400 mb-4"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={1.5}
          d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"
        />
      </svg>

      <p className="text-gray-300 text-lg font-medium mb-2">{message}</p>

      {details && (
        <div className="mb-4">
          <button
            onClick={() => setShowDetails(!showDetails)}
            className="text-xs text-gray-500 hover:text-gray-400 transition-colors"
          >
            {showDetails ? 'Hide details' : 'Show details'}
          </button>
          {showDetails && (
            <pre className="mt-2 p-3 bg-gray-800 rounded text-xs text-gray-400 text-left max-w-lg overflow-auto max-h-32">
              {details}
            </pre>
          )}
        </div>
      )}

      {onRetry && (
        <button
          onClick={onRetry}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          Retry
        </button>
      )}
    </div>
  );
}
