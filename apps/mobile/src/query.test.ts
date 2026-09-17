import { afterEach, expect, it, vi } from 'vitest';
vi.mock('@react-native-async-storage/async-storage', () => ({
  default: {
    getItem: vi.fn(async () => null),
    setItem: vi.fn(async () => {}),
    removeItem: vi.fn(async () => {}),
  },
}));
vi.mock('./session', () => ({
  RequestError: class extends Error {
    status = 0;
  },
}));
import {
  clearPrivateCache,
  queryClient,
  clearHistory,
  rememberProduct,
  rememberSearch,
} from './query';
import type { Product } from '@safeeat/contracts';
afterEach(() => queryClient.clear());
it('bounds and deduplicates history, and clears it without deleting favorites or products', async () => {
  for (let n = 0; n < 65; n++) {
    rememberProduct({ id: `off-${n}` } as Product);
    rememberSearch(`query ${n}`);
  }
  rememberSearch('QUERY 64');
  rememberProduct({ id: 'off-64', name: 'updated' } as Product);
  expect(queryClient.getQueryData<Product[]>(['recent'])).toHaveLength(50);
  expect(queryClient.getQueryData<unknown[]>(['search-history'])).toHaveLength(50);
  queryClient.setQueryData(['favorites-products', 'user'], { pages: [1] });
  queryClient.setQueryData(['product', 'off-64'], { name: 'kept' });
  await clearHistory();
  expect(queryClient.getQueryData(['recent'])).toEqual([]);
  expect(queryClient.getQueryData(['search-history'])).toEqual([]);
  expect(queryClient.getQueryData(['favorites-products', 'user'])).toEqual({ pages: [1] });
  expect(queryClient.getQueryData(['product', 'off-64'])).toEqual({ name: 'kept' });
});
it('clears account data while retaining public products for offline access', async () => {
  const publicProduct = { id: 'off-1', userCreated: false };
  const privateProduct = { id: 'label-1', userCreated: true };
  queryClient.setQueryData(['product', 'off-1'], publicProduct);
  queryClient.setQueryData(['product', 'label-1'], privateProduct);
  queryClient.setQueryData(['recent'], [privateProduct, publicProduct]);
  queryClient.setQueryData(['favorites-products', 'user-1'], { pages: [] });
  await clearPrivateCache();
  expect(queryClient.getQueryData(['product', 'label-1'])).toBeUndefined();
  expect(queryClient.getQueryData(['favorites-products', 'user-1'])).toBeUndefined();
  expect(queryClient.getQueryData(['product', 'off-1'])).toEqual(publicProduct);
  expect(queryClient.getQueryData(['recent'])).toEqual([publicProduct]);
});
it('bounds inactive detail caches while keeping the newest records', () => {
  for (let n = 0; n < 120; n++)
    queryClient.setQueryData(['product', `off-${n}`], { id: n }, { updatedAt: n + 1 });
  expect(queryClient.getQueryCache().getAll()).toHaveLength(100);
  expect(queryClient.getQueryData(['product', 'off-0'])).toBeUndefined();
  expect(queryClient.getQueryData(['product', 'off-119'])).toEqual({ id: 119 });
});
