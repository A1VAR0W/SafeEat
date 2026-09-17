import { useState } from 'react';
import { Pressable, View } from 'react-native';
import Ionicons from '@expo/vector-icons/Ionicons';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { api, useSession } from './session';
import { Copy, ErrorNotice, useTheme } from './ui';

export const collectionSchema = z.object({
  id: z.string().uuid(),
  name: z.string(),
  count: z.number(),
});
export type Collection = z.infer<typeof collectionSchema>;
export function useCollections() {
  const { user } = useSession();
  return useQuery({
    queryKey: ['collections', user?.id],
    enabled: !!user,
    queryFn: async () =>
      z.object({ items: z.array(collectionSchema) }).parse(await api('/collections', {}, true))
        .items,
  });
}
export function CollectionPicker({ productId }: { productId: string }) {
  const { user } = useSession();
  const collections = useCollections();
  const client = useQueryClient();
  const t = useTheme();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const membership = useQuery({
    queryKey: ['collection-membership', user?.id, productId],
    enabled: !!user,
    queryFn: async () =>
      z
        .object({ ids: z.array(z.string().uuid()) })
        .parse(await api(`/collections/for-product/${encodeURIComponent(productId)}`, {}, true))
        .ids,
  });
  if (!user || !collections.data?.length) return null;
  async function toggle(id: string) {
    const included = membership.data?.includes(id) ?? false;
    setBusy(id);
    setError(null);
    try {
      await api(
        `/collections/${id}/products/${encodeURIComponent(productId)}`,
        { method: included ? 'DELETE' : 'POST' },
        true,
      );
      await Promise.all([
        client.invalidateQueries({ queryKey: ['collection-membership', user?.id, productId] }),
        client.invalidateQueries({ queryKey: ['collections', user?.id] }),
        client.invalidateQueries({ queryKey: ['collection-products'] }),
        client.invalidateQueries({ queryKey: ['favorites-products', user?.id] }),
      ]);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(null);
    }
  }
  return (
    <View style={{ gap: 8 }}>
      <Copy bold>Organizar en grupos</Copy>
      {collections.data.map((collection) => {
        const selected = membership.data?.includes(collection.id) ?? false;
        return (
          <Pressable
            key={collection.id}
            accessibilityRole="checkbox"
            accessibilityState={{ checked: selected, disabled: busy === collection.id }}
            accessibilityLabel={`${collection.name}, ${selected ? 'en el grupo' : 'añadir al grupo'}`}
            disabled={!!busy}
            onPress={() => void toggle(collection.id)}
            style={{
              minHeight: 48,
              padding: 12,
              backgroundColor: t.card,
              borderRadius: 14,
              flexDirection: 'row',
              gap: 10,
              alignItems: 'center',
            }}
          >
            <Ionicons name={selected ? 'checkbox' : 'square-outline'} size={23} color={t.accent} />
            <Copy>{collection.name}</Copy>
          </Pressable>
        );
      })}
      {!!error && <ErrorNotice error={error} />}
    </View>
  );
}
