import { ImageIcon } from './icons';

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
        <ImageIcon className="w-16 h-16 mb-4 text-gray-600" />
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
