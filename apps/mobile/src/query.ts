import { QueryClient } from '@tanstack/react-query';
import { createAsyncStoragePersister } from '@tanstack/query-async-storage-persister';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { RequestError } from './session';
import type { Product } from '@safeeat/contracts';
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 300000,
      gcTime: 7 * 86400000,
      retry: (count, error) => count < 1 && error instanceof RequestError && error.status >= 500,
    },
    mutations: { retry: false },
  },
});
queryClient.getQueryCache().subscribe((event) => {
  if (event.type !== 'updated' || event.action.type !== 'success') return;
  const details = queryClient
    .getQueryCache()
    .getAll()
    .filter((q) => ['product', 'barcode'].includes(String(q.queryKey[0])))
    .sort((a, b) => b.state.dataUpdatedAt - a.state.dataUpdatedAt);
  for (const old of details.slice(100)) {
    if (!old.isActive()) queryClient.removeQueries({ queryKey: old.queryKey, exact: true });
  }
});
export const persister = createAsyncStoragePersister({
  storage: AsyncStorage,
  key: 'safeeat.cache.v1',
  throttleTime: 1000,
});
export async function clearPrivateCache() {
  await queryClient.cancelQueries();
  queryClient.removeQueries({
    predicate: (q) => {
      const kind = String(q.queryKey[0]);
      return (
        [
          'favorite',
          'favorites-products',
          'favorites-restaurants',
          'local-label',
          'share',
          'shared-product',
        ].includes(kind) ||
        (kind === 'product' && !!(q.state.data as Product | undefined)?.userCreated)
      );
    },
  });
  queryClient.setQueryData<Product[]>(['recent'], (items) => items?.filter((p) => !p.userCreated));
  await persister.removeClient();
}
export function shouldPersist(query: { queryKey: readonly unknown[]; state: { status: string } }) {
  return (
    query.state.status === 'success' &&
    ['product', 'barcode', 'recent', 'favorites-products', 'favorites-restaurants'].includes(
      String(query.queryKey[0]),
    )
  );
}
