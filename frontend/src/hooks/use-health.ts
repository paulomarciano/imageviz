import { useState, useEffect } from 'react';

interface HealthStatus {
  status: string;
  version: string;
}

interface UseHealthReturn {
  status: string | null;
  isLoading: boolean;
  error: Error | null;
}

export function useHealth(): UseHealthReturn {
  const [status, setStatus] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    fetch('/api/v1/health')
      .then((res) => res.json())
      .then((data: HealthStatus) => setStatus(data.status))
      .catch((err) => setError(err instanceof Error ? err : new Error(String(err))))
      .finally(() => setIsLoading(false));
  }, []);

  return { status, isLoading, error };
}
