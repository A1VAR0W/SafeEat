import { useState } from 'react';
import { ActivityIndicator, FlatList, View, ScrollView, Pressable, Platform } from 'react-native';
import { router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { ProductSchema } from '@safeeat/contracts';
import { z } from 'zod';
import { api } from '../../src/session';
import { rememberSearch, type SearchHistory } from '../../src/query';
import { useDebounced } from '../../src/use-debounced';
import { SearchField, SettingRow } from '../../src/compact';
import { GridPicker, useProductColumns } from '../../src/grid';
import { Copy, ErrorNotice, ProductTile, RecentProducts, Screen, useTheme } from '../../src/ui';
export default function Home() {
  const t = useTheme();
  const { columns, choose } = useProductColumns();
  const [text, setText] = useState('');
  const query = text.trim();
  const settled = useDebounced(query);
  const history = useQuery<SearchHistory[]>({
    queryKey: ['search-history'],
    queryFn: async () => [],
    staleTime: Infinity,
  });
  const search = useQuery({
    queryKey: ['search', query],
    enabled: query.length >= 2 && query === settled,
    queryFn: async ({ signal }) =>
      z
        .object({ items: z.array(ProductSchema) })
        .parse(await api(`/products/search?q=${encodeURIComponent(query)}`, { signal })),
  });
  return (
    <Screen scroll={false}>
      <SearchField
        label="Buscar producto"
        value={text}
        onChangeText={setText}
        placeholder="Buscar producto por nombre"
        maxLength={100}
        onSubmitEditing={() => {
          rememberSearch(query);
          if (query.length >= 2) void search.refetch();
        }}
      />
      <FlatList
        key={`products-${columns}`}
        numColumns={columns}
        columnWrapperStyle={columns > 1 ? { alignItems: 'stretch' } : undefined}
        data={search.data?.items ?? []}
        keyExtractor={(p) => p.id}
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode={Platform.OS === 'ios' ? 'interactive' : 'on-drag'}
        renderItem={({ item }) => (
          <ProductTile item={item} columns={columns} onOpen={() => rememberSearch(query)} />
        )}
        ListHeaderComponent={
          <View style={{ gap: 12, paddingBottom: 12 }}>
            <View style={{ backgroundColor: t.card, borderRadius: 18 }}>
              <SettingRow
                icon="scan-outline"
                title="Escanear producto"
                subtitle="Código de barras o ingredientes"
                onPress={() => router.push('/scan')}
              />
            </View>
            <GridPicker columns={columns} onChange={choose} />
            {query.length >= 2 ? (
              <>
                <Copy bold>Resultados</Copy>
                {(search.isFetching || query !== settled) && (
                  <ActivityIndicator accessibilityLabel="Buscando productos" color={t.accent} />
                )}
                {search.error && (
                  <ErrorNotice error={search.error} retry={() => void search.refetch()} />
                )}
                {search.isSuccess && !search.data.items.length && (
                  <Copy muted>
                    No hay resultados. Prueba el código de barras o escanea los ingredientes.
                  </Copy>
                )}
              </>
            ) : (
              <>
                <Copy bold>Recientes</Copy>
                <RecentProducts columns={columns} />
                {!!history.data?.length && (
                  <ScrollView
                    horizontal
                    showsHorizontalScrollIndicator={false}
                    keyboardShouldPersistTaps="handled"
                    contentContainerStyle={{ gap: 8 }}
                  >
                    {history.data.slice(0, 6).map((h) => (
                      <Pressable
                        key={h.query}
                        onPress={() => setText(h.query)}
                        accessibilityRole="button"
                        style={{
                          padding: 12,
                          minHeight: 48,
                          borderRadius: 16,
                          backgroundColor: t.card,
                        }}
                      >
                        <Copy size={13}>{h.query}</Copy>
                      </Pressable>
                    ))}
                  </ScrollView>
                )}
              </>
            )}
          </View>
        }
      />
    </Screen>
  );
}
