import { useState } from 'react';
import { Alert, FlatList, Pressable, ScrollView, View } from 'react-native';
import { router } from 'expo-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { ProductSchema } from '@safeeat/contracts';
import { SegmentedControl } from '../../src/compact';
import { useCollections } from '../../src/collections';
import { useFavorites } from '../../src/favorites';
import { api } from '../../src/session';
import {
  Button,
  Copy,
  ErrorNotice,
  Field,
  ProductRow,
  Screen,
  Skeleton,
  useTheme,
} from '../../src/ui';
export default function Favorites() {
  const [kind, setKind] = useState<'products' | 'restaurants'>('products');
  const [selected, setSelected] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const { items, query, user } = useFavorites(kind);
  const collections = useCollections();
  const client = useQueryClient();
  const t = useTheme();
  const grouped = useQuery({
    queryKey: ['collection-products', user?.id, selected],
    enabled: !!user && !!selected,
    queryFn: async () =>
      z
        .object({ items: z.array(ProductSchema) })
        .parse(await api(`/collections/${selected}/products`, {}, true)).items,
  });
  async function create() {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const item = z
        .object({ id: z.string().uuid() })
        .parse(
          await api(
            '/collections',
            { method: 'POST', body: JSON.stringify({ name: name.trim() }) },
            true,
          ),
        );
      setName('');
      setSelected(item.id);
      await client.invalidateQueries({ queryKey: ['collections', user?.id] });
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  function removeCollection(id: string, label: string) {
    Alert.alert('Eliminar grupo', `¿Eliminar “${label}”? Los productos seguirán en Guardados.`, [
      { text: 'Cancelar', style: 'cancel' },
      {
        text: 'Eliminar',
        style: 'destructive',
        onPress: () =>
          void (async () => {
            try {
              await api(`/collections/${id}`, { method: 'DELETE' }, true);
              if (selected === id) setSelected(null);
              await client.invalidateQueries({ queryKey: ['collections', user?.id] });
            } catch (e) {
              setError(e);
            }
          })(),
      },
    ]);
  }
  return (
    <Screen scroll={false}>
      <SegmentedControl
        value={kind}
        onChange={setKind}
        options={[
          { value: 'products', label: 'Productos' },
          { value: 'restaurants', label: 'Restaurantes' },
        ]}
      />
      {!user ? (
        <>
          <Copy>Inicia sesión para guardar tus favoritos y etiquetas.</Copy>
          <Button title="Iniciar sesión" onPress={() => router.push('/auth/login')} />
        </>
      ) : (
        <FlatList
          data={kind === 'products' && selected ? (grouped.data ?? []) : items}
          keyExtractor={(i) => i.id}
          onEndReached={() => {
            if (!selected && query.hasNextPage && !query.isFetching) void query.fetchNextPage();
          }}
          renderItem={({ item }) =>
            'name' in item ? (
              <ProductRow item={item} />
            ) : (
              <View style={{ marginBottom: 12 }}>
                <Button
                  secondary
                  title={`Ver restaurante guardado · ${item.id.slice(-6)}`}
                  onPress={() => router.push(`/restaurant/${item.id}`)}
                />
              </View>
            )
          }
          ListEmptyComponent={
            (selected ? grouped.isPending : query.isPending) ? (
              <Skeleton />
            ) : (
              <Copy>
                {selected
                  ? 'Este grupo está vacío. Añade productos desde su ficha.'
                  : `Todavía no has guardado ${kind === 'products' ? 'alimentos' : 'restaurantes'}.`}
              </Copy>
            )
          }
          ListHeaderComponent={
            <View style={{ gap: 10, paddingBottom: 12 }}>
              {kind === 'products' && (
                <>
                  <Copy bold>Mis grupos</Copy>
                  <View style={{ flexDirection: 'row', gap: 8, alignItems: 'flex-end' }}>
                    <View style={{ flex: 1 }}>
                      <Field
                        label="Nuevo grupo"
                        value={name}
                        onChangeText={setName}
                        maxLength={60}
                        placeholder="Desayuno, favoritos…"
                        onSubmitEditing={() => void create()}
                      />
                    </View>
                    <Button
                      title="Crear"
                      disabled={busy || !name.trim()}
                      onPress={() => void create()}
                    />
                  </View>
                  <ScrollView style={{ maxHeight: 210 }} contentContainerStyle={{ gap: 6 }}>
                    <Pressable
                      accessibilityRole="button"
                      accessibilityState={{ selected: !selected }}
                      onPress={() => setSelected(null)}
                      style={{
                        padding: 12,
                        minHeight: 46,
                        borderRadius: 12,
                        backgroundColor: !selected ? t.line : t.card,
                      }}
                    >
                      <Copy bold={!selected}>Todos los productos</Copy>
                    </Pressable>
                    {collections.data?.map((collection) => (
                      <View
                        key={collection.id}
                        style={{
                          flexDirection: 'row',
                          alignItems: 'center',
                          backgroundColor: selected === collection.id ? t.line : t.card,
                          borderRadius: 12,
                        }}
                      >
                        <Pressable
                          accessibilityRole="button"
                          accessibilityState={{ selected: selected === collection.id }}
                          onPress={() => setSelected(collection.id)}
                          style={{ flex: 1, minHeight: 46, padding: 12 }}
                        >
                          <Copy bold={selected === collection.id}>
                            {collection.name} · {collection.count}
                          </Copy>
                        </Pressable>
                        <Pressable
                          accessibilityRole="button"
                          accessibilityLabel={`Eliminar grupo ${collection.name}`}
                          onPress={() => removeCollection(collection.id, collection.name)}
                          style={{ padding: 12 }}
                        >
                          <Copy muted>✕</Copy>
                        </Pressable>
                      </View>
                    ))}
                  </ScrollView>
                </>
              )}
              {!!(error || query.error || grouped.error || collections.error) && (
                <ErrorNotice
                  error={error || query.error || grouped.error || collections.error}
                  retry={() => {
                    void query.refetch();
                    if (selected) void grouped.refetch();
                  }}
                />
              )}
            </View>
          }
          ListFooterComponent={
            !selected && query.hasNextPage ? (
              <Button title="Cargar más" secondary onPress={() => void query.fetchNextPage()} />
            ) : null
          }
        />
      )}
    </Screen>
  );
}
