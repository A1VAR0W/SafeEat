import { useState } from 'react';
import { FlatList, View, Pressable } from 'react-native';
import { router } from 'expo-router';
import * as Location from 'expo-location';
import { useQuery } from '@tanstack/react-query';
import { z } from 'zod';
import { RestaurantSchema, restaurantDisclaimer } from '@safeeat/contracts';
import { useDebounced } from '../../src/use-debounced';
import { SearchField } from '../../src/compact';
import { api } from '../../src/session';
import { Button, Card, Copy, ErrorNotice, Screen, Skeleton, Title, useTheme } from '../../src/ui';
export default function Restaurants() {
  const theme = useTheme();
  const [text, setText] = useState('');
  const [params, setParams] = useState<{ q: string; latitude?: number; longitude?: number } | null>(
    null,
  );
  const [locationError, setLocationError] = useState<unknown>(null);
  const [locating, setLocating] = useState(false);
  const settled = useDebounced(text.trim());
  const suggestions = useQuery({
    queryKey: ['restaurant-suggestions', settled],
    enabled: settled.length >= 3 && !params,
    queryFn: async ({ signal }) =>
      z
        .object({
          items: z.array(
            z.object({
              label: z.string(),
              kind: z.enum(['restaurant', 'city']),
              query: z.string(),
              latitude: z.number().optional(),
              longitude: z.number().optional(),
            }),
          ),
        })
        .parse(await api(`/restaurants/suggest?q=${encodeURIComponent(settled)}`, { signal })),
    staleTime: 3600000,
  });
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
        q: 'restaurantes',
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
                {item.source}
              </Copy>
            </Card>
          </Pressable>
        )}
        ListHeaderComponent={
          <View style={{ gap: 18, paddingBottom: 18 }}>
            <Title>Comer fuera</Title>
            <Copy muted>{restaurantDisclaimer}</Copy>
            <SearchField
              label="Restaurante o ciudad"
              value={text}
              onChangeText={(value) => {
                setText(value);
                setParams(null);
              }}
              placeholder="Nombre del restaurante, Madrid…"
              onSubmitEditing={() => {
                if (text.trim().length >= 2) setParams({ q: text.trim() });
              }}
            />
            {!params && !!suggestions.data?.items.length && (
              <View style={{ backgroundColor: theme.card, borderRadius: 16, overflow: 'hidden' }}>
                {suggestions.data.items.map((suggestion, index) => (
                  <Pressable
                    key={`${suggestion.kind}-${suggestion.label}-${index}`}
                    accessibilityRole="button"
                    accessibilityLabel={`Buscar ${suggestion.label}`}
                    onPress={() => {
                      setText(suggestion.label);
                      setParams({
                        q: suggestion.query,
                        latitude: suggestion.latitude,
                        longitude: suggestion.longitude,
                      });
                    }}
                    style={{ padding: 14, minHeight: 48 }}
                  >
                    <Copy>
                      {suggestion.kind === 'city' ? '⌖' : '⌕'} {suggestion.label}
                    </Copy>
                  </Pressable>
                ))}
              </View>
            )}
            <Button
              secondary
              title={locating ? 'Buscando ubicación…' : 'Buscar cerca de mí'}
              disabled={locating}
              onPress={() => void nearby()}
            />
            {!!locationError && <ErrorNotice error={locationError} />}
            <Copy muted size={12}>
              Datos de OpenStreetMap o Google Maps según disponibilidad. La ubicación solo se
              comparte al pulsar “Buscar cerca de mí”.
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
