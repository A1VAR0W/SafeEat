import { useState } from 'react';
import { FlatList, View } from 'react-native';
import { router } from 'expo-router';
import { useFavorites } from '../../src/favorites';
import { Button, Copy, ErrorNotice, ProductRow, Screen, Skeleton, Title } from '../../src/ui';
export default function Favorites() {
  const [kind, setKind] = useState<'products' | 'restaurants'>('products');
  const { items, query, user } = useFavorites(kind);
  return (
    <Screen scroll={false}>
      <Title>Tu selección</Title>
      <View style={{ flexDirection: 'row', gap: 12 }}>
        <View style={{ flex: 1 }}>
          <Button
            secondary={kind !== 'products'}
            title="Alimentos"
            onPress={() => setKind('products')}
          />
        </View>
        <View style={{ flex: 1 }}>
          <Button
            secondary={kind !== 'restaurants'}
            title="Restaurantes"
            onPress={() => setKind('restaurants')}
          />
        </View>
      </View>
      {!user ? (
        <>
          <Copy>Inicia sesión para guardar tus favoritos y etiquetas.</Copy>
          <Button title="Iniciar sesión" onPress={() => router.push('/auth/login')} />
        </>
      ) : (
        <FlatList
          data={items}
          keyExtractor={(i) => i.id}
          onEndReached={() => {
            if (query.hasNextPage && !query.isFetching) void query.fetchNextPage();
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
            query.isPending ? (
              <Skeleton />
            ) : (
              <Copy>
                Todavía no has guardado {kind === 'products' ? 'alimentos' : 'restaurantes'}.
              </Copy>
            )
          }
          ListHeaderComponent={
            query.error ? (
              <ErrorNotice error={query.error} retry={() => void query.refetch()} />
            ) : null
          }
          ListFooterComponent={
            query.hasNextPage ? (
              <Button title="Cargar más" secondary onPress={() => void query.fetchNextPage()} />
            ) : null
          }
        />
      )}
    </Screen>
  );
}
