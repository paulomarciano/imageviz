# Wave 6.6 — Build Error Boundary and Error States

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 06 |
| **Estimate** | 1.5 hours |
| **Depends on** | None (independent shared component) |
| **Parallel** | Yes — can run in parallel with other Wave 6 tasks |

---

## Overview

Build an Error Boundary component that catches unhandled React errors and displays a fallback UI. Also build a reusable Error State component for API/network errors with retry functionality.

## Prerequisites

- React 19 (Error Boundary support)
- Tailwind configured (0.3)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.6
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/shared/
├── error-boundary.tsx           # React error boundary
├── error-state.tsx              # API error display with retry
└── __tests__/
    └── error-state.test.tsx     # Component tests
```

## Acceptance Criteria (Pass/Fail)

**Error Boundary:**
- [ ] Catches unhandled React errors in child components
- [ ] Displays fallback UI instead of blank white screen
- [ ] Shows error message and "Try Again" button that remounts children
- [ ] Logs error details to console (or future logging service)
- [ ] Wraps the entire app or critical sections

**Error State:**
- [ ] Props: `message: string`, `onRetry?: () => void`, `details?: string`
- [ ] Displays error icon, message, and retry button
- [ ] Retry button is prominent and accessible
- [ ] "Show details" toggle for technical error info (stack trace, status code)
- [ ] Used by: grid (API failure), search (failed search), detail view (failed fetch)

## Implementation Notes

**Error Boundary (React 19 class component — still required):**
```tsx
import { Component, type ErrorInfo, type ReactNode } from 'react';

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Unhandled error:', error, errorInfo);
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;
      
      return (
        <div className="h-full flex items-center justify-center p-8">
          <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 max-w-md text-center">
            <div className="text-red-400 text-4xl mb-3">⚠</div>
            <h2 className="text-lg font-semibold text-white mb-2">Something went wrong</h2>
            <p className="text-gray-400 text-sm mb-4">
              {this.state.error?.message || 'An unexpected error occurred'}
            </p>
            <button
              onClick={this.handleReset}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded 
                         transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              Try Again
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
```

**Error State component:**
```tsx
import { useState } from 'react';

interface ErrorStateProps {
  message: string;
  details?: string;
  onRetry?: () => void;
}

export function ErrorState({ message, details, onRetry }: ErrorStateProps) {
  const [showDetails, setShowDetails] = useState(false);

  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <svg className="w-12 h-12 text-red-400 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
          d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
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
          className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded 
                     transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          Retry
        </button>
      )}
    </div>
  );
}
```

**Usage:**
```tsx
// Wrap the app
<ErrorBoundary>
  <AppShell>
    <ThumbnailGrid />
  </AppShell>
</ErrorBoundary>

// For API errors in components
<ErrorState
  message="Failed to load media"
  details={error.message}
  onRetry={() => refetch()}
/>
```

## Test Strategy

```tsx
// Error boundary test
function ThrowError() {
  throw new Error('Test error');
}

it('catches errors and shows fallback', () => {
  const { container } = render(
    <ErrorBoundary>
      <ThrowError />
    </ErrorBoundary>
  );
  expect(screen.getByText('Something went wrong')).toBeInTheDocument();
});

// Error state test
it('shows retry button and handles click', () => {
  const onRetry = vi.fn();
  render(<ErrorState message="Failed" onRetry={onRetry} />);
  
  fireEvent.click(screen.getByText('Retry'));
  expect(onRetry).toHaveBeenCalled();
});
```
