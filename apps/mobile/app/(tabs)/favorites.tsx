import { memo, useState, useCallback } from 'react';
import { Alert, FlatList, Pressable, View } from 'react-native';
import { router, useFocusEffect } from 'expo-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { ProductSchema, RestaurantSchema } from '@safeeat/contracts';
import { SegmentedControl } from '../../src/compact';
import { useCollections } from '../../src/collections';
import { useFavorites } from '../../src/favorites';
import { api } from '../../src/session';
import { GridPicker, useProductColumns } from '../../src/grid';
import { Button, Copy, ErrorNotice, ProductTile, Screen, Skeleton, useTheme } from '../../src/ui';
export default function Favorites() {
  const [active, setActive] = useState(false);
  useFocusEffect(
    useCallback(() => {
      setActive(true);
      return () => setActive(false);
    }, []),
  );
  const [kind, setKind] = useState<'products' | 'restaurants'>('products');
  const [selected, setSelected] = useState('favorites');
  const { columns, choose } = useProductColumns();
  const [error, setError] = useState<unknown>(null);
  const { items, query, user } = useFavorites(kind, undefined, active);
  const collections = useCollections();
  const client = useQueryClient();
  const t = useTheme();
  const activeCollection = kind === 'products' && selected !== 'favorites';
  const grouped = useQuery({
    queryKey: ['collection-products', user?.id, selected],
    enabled: active && !!user && activeCollection,
    queryFn: async () =>
      z
        .object({ items: z.array(ProductSchema) })
        .parse(await api(`/collections/${selected}/products`, {}, true)).items,
  });
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
              if (selected === id) setSelected('favorites');
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
          key={`${kind}-${columns}`}
          numColumns={kind === 'products' ? columns : 1}
          columnWrapperStyle={kind === 'products' && columns > 1 ? {} : undefined}
          data={activeCollection ? (grouped.data ?? []) : items}
          initialNumToRender={6}
          maxToRenderPerBatch={6}
          windowSize={5}
          keyExtractor={(i) => i.id}
          onEndReached={() => {
            if (!activeCollection && query.hasNextPage && !query.isFetching)
              void query.fetchNextPage();
          }}
          renderItem={({ item }) =>
            'name' in item ? (
              <ProductTile item={item} columns={columns} />
            ) : (
              <SavedRestaurant id={item.id} active={active} />
            )
          }
          ListEmptyComponent={
            (activeCollection ? grouped.isPending : query.isPending) ? (
              <Skeleton />
            ) : (
              <Copy>
                {activeCollection
                  ? 'Este grupo está vacío. Añade productos desde su ficha.'
                  : `Todavía no has guardado ${kind === 'products' ? 'alimentos' : 'restaurantes'}.`}
              </Copy>
            )
          }
          ListHeaderComponent={
            <View style={{ gap: 10, paddingBottom: 12 }}>
              {kind === 'products' && (
                <>
                  <View
                    style={{
                      flexDirection: 'row',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                    }}
                  >
                    <Copy bold>Mis marcadores</Copy>
                    <Pressable
                      accessibilityRole="button"
                      accessibilityLabel="Crear marcador"
                      onPress={() => router.push('/collection/new')}
                      style={{
                        minWidth: 48,
                        minHeight: 48,
                        borderRadius: 14,
                        backgroundColor: t.accent,
                        alignItems: 'center',
                        justifyContent: 'center',
                      }}
                    >
                      <Copy bold size={25}>
                        +
                      </Copy>
                    </Pressable>
                  </View>
                  <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 10 }}>
                    <Pressable
                      accessibilityRole="button"
                      accessibilityState={{ selected: selected === 'favorites' }}
                      onPress={() => setSelected('favorites')}
                      style={{
                        padding: 15,
                        minHeight: 90,
                        width: '48%',
                        borderRadius: 18,
                        backgroundColor: selected === 'favorites' ? t.line : t.card,
                        justifyContent: 'space-between',
                      }}
                    >
                      <Copy size={23}>♥</Copy>
                      <Copy bold>Favoritos</Copy>
                    </Pressable>
                    {collections.data?.map((collection) => (
                      <View
                        key={collection.id}
                        style={{
                          width: '48%',
                          minHeight: 90,
                          backgroundColor: selected === collection.id ? t.line : t.card,
                          borderRadius: 18,
                        }}
                      >
                        <Pressable
                          accessibilityRole="button"
                          accessibilityState={{ selected: selected === collection.id }}
                          onPress={() => setSelected(collection.id)}
                          style={{ flex: 1, padding: 14, justifyContent: 'space-between' }}
                        >
                          <Copy size={23}>▤</Copy>
                          <Copy bold={selected === collection.id}>
                            {collection.name} · {collection.count}
                          </Copy>
                        </Pressable>
                        <Pressable
                          accessibilityRole="button"
                          accessibilityLabel={`Eliminar grupo ${collection.name}`}
                          onPress={() => removeCollection(collection.id, collection.name)}
                          style={{ position: 'absolute', right: 0, top: 0, padding: 10 }}
                        >
                          <Copy muted>✕</Copy>
                        </Pressable>
                      </View>
                    ))}
                  </View>
                  <GridPicker columns={columns} onChange={choose} />
                </>
              )}
              {!!(error || query.error || grouped.error || collections.error) && (
                <ErrorNotice
                  error={error || query.error || grouped.error || collections.error}
                  retry={() => {
                    void query.refetch();
                    if (activeCollection) void grouped.refetch();
                  }}
                />
              )}
            </View>
          }
          ListFooterComponent={
            !activeCollection && query.hasNextPage ? (
              <Button title="Cargar más" secondary onPress={() => void query.fetchNextPage()} />
            ) : null
          }
        />
      )}
    </Screen>
  );
}
const SavedRestaurant = memo(function SavedRestaurant({
  id,
  active,
}: {
  id: string;
  active: boolean;
}) {
  const t = useTheme();
  const details = useQuery({
    queryKey: ['restaurant', id],
    enabled: active,
    queryFn: async () =>
      RestaurantSchema.parse(await api(`/restaurants/${encodeURIComponent(id)}`)),
  });
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`Ver ${details.data?.name ?? 'restaurante guardado'}`}
      onPress={() => router.push(`/restaurant/${id}`)}
      style={{ marginBottom: 12 }}
    >
      <View style={{ padding: 16, borderRadius: 18, backgroundColor: t.card }}>
        <Copy bold>{details.data?.name ?? 'Restaurante guardado'}</Copy>
        {!!details.data?.address && <Copy muted>{details.data.address}</Copy>}
      </View>
    </Pressable>
  );
});
