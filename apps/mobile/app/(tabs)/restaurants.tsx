import { useState } from 'react';
import { FlatList, View, Pressable } from 'react-native';
import { router } from 'expo-router';
import * as Location from 'expo-location';
import { useQuery } from '@tanstack/react-query';
import { z } from 'zod';
import { RestaurantSchema, restaurantDisclaimer } from '@safeeat/contracts';
import { api } from '../../src/session';
import { Button, Card, Copy, ErrorNotice, Field, Screen, Skeleton, Title } from '../../src/ui';
export default function Restaurants() {
  const [text, setText] = useState('');
  const [params, setParams] = useState<{ q: string; latitude?: number; longitude?: number } | null>(
    null,
  );
  const [locationError, setLocationError] = useState<unknown>(null);
  const [locating, setLocating] = useState(false);
  const search = useQuery({
    queryKey: ['restaurants', params],
    enabled: !!params,
    queryFn: async () => {
      const query = new URLSearchParams({
        q: params!.q,
        ...(params!.latitude !== undefined
          ? { latitude: String(params!.latitude), longitude: String(params!.longitude) }
          : {}),
      });
      return z
        .object({ items: z.array(RestaurantSchema) })
        .parse(await api(`/restaurants/search?${query}`));
    },
    gcTime: 0,
  });
  async function nearby() {
    setLocating(true);
    setLocationError(null);
    try {
      const permission = await Location.requestForegroundPermissionsAsync();
      if (!permission.granted)
        throw new Error('Puedes seguir buscando por nombre y ciudad sin compartir tu ubicación.');
      const pos = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
      setParams({
        q: text.trim() || 'restaurantes',
        latitude: pos.coords.latitude,
        longitude: pos.coords.longitude,
      });
    } catch (e) {
      setLocationError(e);
    } finally {
      setLocating(false);
    }
  }
  return (
    <Screen scroll={false}>
      <FlatList
        data={search.data?.items ?? []}
        keyExtractor={(r) => r.id}
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Ver ${item.name}`}
            onPress={() => router.push(`/restaurant/${item.id}`)}
            style={{ marginBottom: 12 }}
          >
            <Card>
              <Copy bold size={18}>
                {item.name}
              </Copy>
              <Copy muted>{item.address}</Copy>
              <Copy>? Información sobre gluten no verificada</Copy>
              <Copy size={12} muted>
                Google Maps
              </Copy>
            </Card>
          </Pressable>
        )}
        ListHeaderComponent={
          <View style={{ gap: 18, paddingBottom: 18 }}>
            <Title>Comer fuera</Title>
            <Copy muted>{restaurantDisclaimer}</Copy>
            <Field
              label="Restaurante o ciudad"
              value={text}
              onChangeText={setText}
              placeholder="Nombre del restaurante, Madrid…"
              onSubmitEditing={() => setParams({ q: text.trim() })}
            />
            <Button
              title="Buscar restaurantes"
              disabled={text.trim().length < 2 || search.isFetching}
              onPress={() => setParams({ q: text.trim() })}
            />
            <Button
              secondary
              title={locating ? 'Buscando ubicación…' : 'Buscar cerca de mí'}
              disabled={locating}
              onPress={() => void nearby()}
            />
            {!!locationError && <ErrorNotice error={locationError} />}
            <Copy muted size={12}>
              La búsqueda se envía a Google Maps. La ubicación solo se comparte al pulsar “Buscar
              cerca de mí”.
            </Copy>
            {search.isFetching && <Skeleton />}
            {search.error && (
              <ErrorNotice error={search.error} retry={() => void search.refetch()} />
            )}
            {search.isSuccess && !search.data.items.length && (
              <Copy>No se han encontrado restaurantes. Prueba otra búsqueda.</Copy>
            )}
          </View>
        }
      />
    </Screen>
  );
}
