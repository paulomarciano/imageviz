interface EmptyStateAction {
  readonly label: string;
  readonly onClick: () => void;
}

interface EmptyStateProps {
  readonly message: string;
  readonly description?: string;
  readonly icon?: React.ReactNode;
  readonly action?: EmptyStateAction;
}

export function EmptyState({ message, description, icon, action }: EmptyStateProps) {
  return (
    <div className="h-full flex flex-col items-center justify-center text-center p-8">
      {icon ? (
        <div className="mb-4 text-gray-600">{icon}</div>
      ) : (
        <svg
          className="w-16 h-16 mb-4 text-gray-600"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1}
            d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
          />
        </svg>
      )}
      <p className="text-gray-400 text-lg font-medium mb-1">{message}</p>
      {description && <p className="text-gray-500 text-sm max-w-md">{description}</p>}
      {action && (
        <button
          type="button"
          onClick={action.onClick}
          className="mt-4 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
