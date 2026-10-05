import { useState, useCallback, useEffect } from 'react';
import { Image } from 'expo-image';
import { ActivityIndicator, FlatList, View, ScrollView, Pressable, Platform } from 'react-native';
import { router, useFocusEffect } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { ProductSchema, type Product } from '@safeeat/contracts';
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
  const [active, setActive] = useState(false);
  useFocusEffect(
    useCallback(() => {
      setActive(true);
      return () => setActive(false);
    }, []),
  );
  const query = text.trim();
  const settled = useDebounced(query);
  const rememberOpened = useCallback(() => rememberSearch(settled), [settled]);
  const renderProduct = useCallback(
    ({ item }: { item: Product }) => (
      <ProductTile item={item} columns={columns} onOpen={rememberOpened} />
    ),
    [columns, rememberOpened],
  );
  const history = useQuery<SearchHistory[]>({
    queryKey: ['search-history'],
    queryFn: async () => [],
    staleTime: Infinity,
  });
  const search = useQuery({
    queryKey: ['search', settled],
    enabled: active && query.length >= 2 && query === settled,
    placeholderData: (previous) => previous,
    gcTime: 300000,
    queryFn: async ({ signal }) =>
      z
        .object({ items: z.array(ProductSchema) })
        .parse(await api(`/products/search?q=${encodeURIComponent(settled)}`, { signal })),
  });
  useEffect(() => {
    if (!active) return;
    const urls = [
      ...new Set(
        search.data?.items.slice(0, 8).flatMap((item) => (item.imageUrl ? [item.imageUrl] : [])) ??
          [],
      ),
    ];
    if (urls.length) void Image.prefetch(urls, 'memory-disk').catch(() => {});
  }, [search.data, active]);
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
          if (query.length >= 2 && query === settled) void search.refetch();
        }}
      />
      <FlatList
        key={`products-${columns}`}
        numColumns={columns}
        columnWrapperStyle={columns > 1 ? { alignItems: 'stretch' } : undefined}
        data={query.length >= 2 ? (search.data?.items ?? []) : []}
        initialNumToRender={6}
        maxToRenderPerBatch={6}
        windowSize={5}
        keyExtractor={(p) => p.id}
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode={Platform.OS === 'ios' ? 'interactive' : 'on-drag'}
        renderItem={renderProduct}
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
                {search.isSuccess && !search.isPlaceholderData && !search.data.items.length && (
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
