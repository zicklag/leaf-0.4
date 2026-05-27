import { QueryClient, QueryObserver, MutationObserver } from '@tanstack/svelte-query';
import { readable } from 'svelte/store';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: true,
    },
    mutations: {
      retry: 0,
    },
  },
});

/**
 * Create a Svelte readable store from a TanStack Query query.
 * Auto-subscribes and unsubscribes.
 */
export function queryStore<T>(options: {
  queryKey: unknown[];
  queryFn: () => Promise<T>;
  enabled?: boolean;
  staleTime?: number;
}) {
  const observer = new QueryObserver(queryClient, {
    queryKey: options.queryKey,
    queryFn: options.queryFn,
    staleTime: options.staleTime ?? 30_000,
    enabled: options.enabled ?? true,
  });

  const store = readable(observer.getCurrentResult(), (set) => {
    return observer.subscribe((result) => {
      set(result);
    });
  });

  return store;
}

/**
 * Create a mutation function + store pair.
 */
export function createMutation<TData, TVariables>(options: {
  mutationFn: (variables: TVariables) => Promise<TData>;
  onSuccess?: (data: TData, variables: TVariables) => void;
  onError?: (error: Error, variables: TVariables) => void;
  invalidateQueries?: unknown[][];
}) {
  const observer = new MutationObserver(queryClient, {
    mutationFn: options.mutationFn,
    onSuccess: (data, variables) => {
      if (options.invalidateQueries) {
        for (const key of options.invalidateQueries) {
          queryClient.invalidateQueries({ queryKey: key });
        }
      }
      options.onSuccess?.(data, variables);
    },
    onError: options.onError,
  });

  const store = readable(observer.getCurrentResult(), (set) => {
    return observer.subscribe((result) => {
      set(result);
    });
  });

  const mutate = (variables: TVariables) => {
    observer.mutate(variables);
  };

  return { mutate, store };
}
