import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { z } from 'zod';
import { useMemo } from 'react';
import { api, useSession } from './session';
type Page<T> = { items: T[]; nextCursor: string | null };
export function useFavorites(kind: 'products' | 'restaurants', itemId?: string, active = true) {
  const { user } = useSession();
  const client = useQueryClient();
  const key = [`favorites-${kind}`, user?.id];
  const membershipKey = ['favorite', kind, user?.id, itemId];
  const membership = useQuery({
    queryKey: membershipKey,
    enabled: !!user && !!itemId,
    queryFn: async () =>
      z
        .object({ saved: z.boolean() })
        .parse(await api(`/favorites/${kind}/${encodeURIComponent(itemId!)}`, {}, true)),
  });
  const query = useInfiniteQuery({
    queryKey: key,
    enabled: !!user && !itemId && active,
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) => {
      const raw = await api(
        `/favorites/${kind}${pageParam ? `?cursor=${encodeURIComponent(pageParam)}` : ''}`,
        {},
        true,
      );
      const schema = z.object({
        items: z.array(kind === 'products' ? ProductSchema : z.object({ id: z.string() })),
        nextCursor: z.string().nullable(),
      });
      return schema.parse(raw) as Page<Product | { id: string }>;
    },
    getNextPageParam: (last) => last.nextCursor ?? undefined,
  });
  const mutation = useMutation({
    mutationFn: async ({ item, remove }: { item: Product | { id: string }; remove: boolean }) =>
      api(
        `/favorites/${kind}/${encodeURIComponent(item.id)}`,
        { method: remove ? 'DELETE' : 'POST' },
        true,
      ),
    onMutate: async ({ item, remove }) => {
      await client.cancelQueries({ queryKey: key });
      await client.cancelQueries({ queryKey: membershipKey });
      const previousMembership = client.getQueryData(membershipKey);
      if (itemId) client.setQueryData(membershipKey, { saved: !remove });
      const previous = client.getQueryData<{
        pages: Page<Product | { id: string }>[];
        pageParams: (string | null)[];
      }>(key);
      client.setQueryData(key, (old: typeof previous) => {
        const data = old ?? { pages: [{ items: [], nextCursor: null }], pageParams: [null] };
        return {
          ...data,
          pages: data.pages.map((p, i) => ({
            ...p,
            items: remove
              ? p.items.filter((x) => x.id !== item.id)
              : i === 0
                ? [item, ...p.items.filter((x) => x.id !== item.id)]
                : p.items,
          })),
        };
      });
      return { previous, previousMembership };
    },
    onError: (_error, _variables, context) => {
      if (context?.previous) client.setQueryData(key, context.previous);
      else client.removeQueries({ queryKey: key, exact: true });
      if (itemId) client.setQueryData(membershipKey, context?.previousMembership);
    },
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: key });
      await client.invalidateQueries({ queryKey: ['favorite', kind, user?.id] });
    },
  });
  const items = useMemo(() => query.data?.pages.flatMap((p) => p.items) ?? [], [query.data]);
  return {
    query,
    membership,
    mutation,
    items,
    user,
  };
}
