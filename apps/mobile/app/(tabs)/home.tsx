import { useState } from 'react';
import { FlatList, View } from 'react-native';
import { router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { ProductSchema } from '@safeeat/contracts';
import { z } from 'zod';
import { api } from '../../src/session';
import {
  Button,
  Copy,
  ErrorNotice,
  Field,
  ProductRow,
  RecentProducts,
  Screen,
  Skeleton,
  Title,
  Card,
} from '../../src/ui';
export default function Home() {
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const search = useQuery({
    queryKey: ['search', query],
    enabled: query.length >= 2,
    queryFn: async () =>
      z
        .object({ items: z.array(ProductSchema) })
        .parse(await api(`/products/search?q=${encodeURIComponent(query)}`)),
  });
  return (
    <Screen scroll={false}>
      <FlatList
        data={search.data?.items ?? []}
        keyExtractor={(p) => p.id}
        keyboardShouldPersistTaps="handled"
        renderItem={({ item }) => <ProductRow item={item} />}
        ListHeaderComponent={
          <View style={{ gap: 20, paddingBottom: 16 }}>
            <Copy bold size={18}>
              SafeEat
            </Copy>
            <Title>¿Qué vas{'\n'}a comer?</Title>
            <Card>
              <Copy size={18} bold>
                Más información. Menos dudas.
              </Copy>
              <Copy muted>Lee la etiqueta y conoce la evidencia detrás de cada resultado.</Copy>
              <Button title="▥  Escanear producto" onPress={() => router.push('/(tabs)/scan')} />
            </Card>
            <Field
              label="Buscar producto"
              value={text}
              onChangeText={setText}
              placeholder="Nombre, marca…"
              returnKeyType="search"
              onSubmitEditing={() => setQuery(text.trim())}
            />
            <Button
              title="Buscar"
              secondary
              disabled={text.trim().length < 2 || search.isFetching}
              onPress={() => setQuery(text.trim())}
            />
            {search.isFetching && <Skeleton />}
            {search.error && (
              <ErrorNotice error={search.error} retry={() => void search.refetch()} />
            )}
            <Copy bold size={21}>
              {query ? 'Resultados' : 'Escaneados recientemente'}
            </Copy>
            {!query && <RecentProducts />}
            {query && search.isSuccess && !search.data.items.length && (
              <Copy muted>
                No hemos encontrado productos. Prueba con el código de barras o escanea los
                ingredientes.
              </Copy>
            )}
          </View>
        }
      />
    </Screen>
  );
}
