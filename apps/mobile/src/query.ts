import { QueryClient } from '@tanstack/react-query';
import { createAsyncStoragePersister } from '@tanstack/query-async-storage-persister';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { RequestError } from './session';
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
export const persister = createAsyncStoragePersister({
  storage: AsyncStorage,
  key: 'safeeat.cache.v1',
  throttleTime: 1000,
});
export async function clearPrivateCache() {
  queryClient.clear();
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
