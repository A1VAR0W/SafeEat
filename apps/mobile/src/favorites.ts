import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { z } from 'zod';
import { api, useSession } from './session';
type Page<T> = { items: T[]; nextCursor: string | null };
export function useFavorites(kind: 'products' | 'restaurants') {
  const { user } = useSession();
  const client = useQueryClient();
  const key = [`favorites-${kind}`, user?.id];
  const query = useInfiniteQuery({
    queryKey: key,
    enabled: !!user,
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
      return { previous };
    },
    onError: (_error, _variables, context) => {
      if (context?.previous) client.setQueryData(key, context.previous);
      else client.removeQueries({ queryKey: key, exact: true });
    },
    onSettled: () => client.invalidateQueries({ queryKey: key }),
  });
  return { query, mutation, items: query.data?.pages.flatMap((p) => p.items) ?? [], user };
}
