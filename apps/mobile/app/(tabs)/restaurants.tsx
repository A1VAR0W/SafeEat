import { useEffect, useRef, useState } from 'react';
import { FlatList, Platform, Pressable, View } from 'react-native';
import { router } from 'expo-router';
import * as Location from 'expo-location';
import { AppleMaps, GoogleMaps } from 'expo-maps';
import { useQuery } from '@tanstack/react-query';
import { z } from 'zod';
import { RestaurantSchema, restaurantDisclaimer, type Restaurant } from '@safeeat/contracts';
import { useDebounced } from '../../src/use-debounced';
import { SearchField } from '../../src/compact';
import { api } from '../../src/session';
import { Button, Card, Copy, ErrorNotice, Screen, Title, useTheme } from '../../src/ui';

type Coordinates = { latitude: number; longitude: number };
type SearchParams = { q: string; latitude?: number; longitude?: number };
const initialCenter = { latitude: 40.4168, longitude: -3.7038 };
const androidMapsConfigured = !!process.env.EXPO_PUBLIC_GOOGLE_MAPS_ANDROID_API_KEY;

export default function Restaurants() {
  const theme = useTheme();
  const appleMap = useRef<AppleMaps.MapView>(null);
  const googleMap = useRef<GoogleMaps.MapView>(null);
  const [text, setText] = useState('');
  const [params, setParams] = useState<SearchParams | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mapCenter, setMapCenter] = useState<Coordinates>(initialCenter);
  const [searchedCenter, setSearchedCenter] = useState<Coordinates | null>(null);
  const [locationError, setLocationError] = useState<unknown>(null);
  const [locating, setLocating] = useState(false);
  const [glutenOnly, setGlutenOnly] = useState(false);
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
    queryKey: ['restaurants', params, glutenOnly],
    enabled: !!params,
    queryFn: async () => {
      const query = new URLSearchParams({
        q: params!.q,
        ...(params!.latitude !== undefined
          ? { latitude: String(params!.latitude), longitude: String(params!.longitude) }
          : {}),
        ...(glutenOnly ? { glutenFree: 'true' } : {}),
      });
      return z
        .object({ items: z.array(RestaurantSchema) })
        .parse(await api(`/restaurants/search?${query}`));
    },
    gcTime: 0,
  });
  const items = (search.data?.items ?? []).filter(
    (item) =>
      !glutenOnly ||
      item.glutenSupport === 'DEDICATED_GLUTEN_FREE' ||
      item.glutenSupport === 'GLUTEN_FREE_OPTIONS',
  );
  const mapped = items.filter(
    (item): item is Restaurant & Coordinates =>
      item.latitude !== undefined && item.longitude !== undefined,
  );
  const selected = items.find((item) => item.id === selectedId);

  function focus(coordinates: Coordinates, zoom = 15) {
    appleMap.current?.setCameraPosition({ coordinates, zoom });
    googleMap.current?.setCameraPosition({ coordinates, zoom, duration: 300 });
    setMapCenter(coordinates);
  }

  useEffect(() => {
    if (!search.isSuccess) return;
    const first = search.data.items.find(
      (item) => item.latitude !== undefined && item.longitude !== undefined,
    );
    const center =
      params?.latitude !== undefined && params.longitude !== undefined
        ? { latitude: params.latitude, longitude: params.longitude }
        : first?.latitude !== undefined && first.longitude !== undefined
          ? { latitude: first.latitude, longitude: first.longitude }
          : null;
    setSelectedId(null);
    if (center) {
      focus(center, params?.latitude !== undefined ? 15 : 13);
      setSearchedCenter(center);
    }
  }, [params?.latitude, params?.longitude, search.data, search.isSuccess]);

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
      focus({ latitude: pos.coords.latitude, longitude: pos.coords.longitude });
    } catch (e) {
      setLocationError(e);
    } finally {
      setLocating(false);
    }
  }

  function select(item: Restaurant) {
    setSelectedId(item.id);
    if (item.latitude !== undefined && item.longitude !== undefined)
      focus({ latitude: item.latitude, longitude: item.longitude }, 17);
  }

  const mapAvailable =
    Platform.OS === 'ios' || (Platform.OS === 'android' && androidMapsConfigured);
  const searchOrigin = searchedCenter ?? initialCenter;
  const canSearchArea =
    mapAvailable &&
    (Math.abs(mapCenter.latitude - searchOrigin.latitude) > 0.003 ||
      Math.abs(mapCenter.longitude - searchOrigin.longitude) > 0.003);

  return (
    <Screen scroll={false}>
      <Title>Comer fuera</Title>
      <SearchField
        label="Restaurante o ciudad"
        value={text}
        onChangeText={(value) => {
          setText(value);
          setParams(null);
        }}
        placeholder="Restaurante o ciudad…"
        onSubmitEditing={() => {
          if (text.trim().length >= 2) setParams({ q: text.trim() });
        }}
      />
      {!params && !!suggestions.data?.items.length && (
        <View style={{ backgroundColor: theme.card, borderRadius: 16, overflow: 'hidden' }}>
          {suggestions.data.items.slice(0, 4).map((suggestion, index) => (
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
              style={{ padding: 12, minHeight: 48 }}
            >
              <Copy>
                {suggestion.kind === 'city' ? '⌖' : '⌕'} {suggestion.label}
              </Copy>
            </Pressable>
          ))}
        </View>
      )}
      <View style={{ flexDirection: 'row', gap: 10 }}>
        <View style={{ flex: 1 }}>
          <Button
            secondary
            title={locating ? 'Localizando…' : 'Cerca de mí'}
            disabled={locating}
            onPress={() => void nearby()}
          />
        </View>
        {canSearchArea && (
          <View style={{ flex: 1 }}>
            <Button
              secondary
              title="Buscar en esta zona"
              onPress={() =>
                setParams({
                  q: 'restaurantes',
                  latitude: mapCenter.latitude,
                  longitude: mapCenter.longitude,
                })
              }
            />
          </View>
        )}
      </View>
      {!!locationError && <ErrorNotice error={locationError} />}
      <View style={{ flexDirection: 'row', gap: 8, alignItems: 'center' }}>
        <Pressable
          accessibilityRole="switch"
          accessibilityState={{ checked: glutenOnly }}
          onPress={() => setGlutenOnly((value) => !value)}
          style={{
            padding: 10,
            borderRadius: 14,
            backgroundColor: glutenOnly ? theme.accent : theme.card,
          }}
        >
          <Copy bold size={13}>
            ✓ Opciones sin gluten
          </Copy>
        </Pressable>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Añadir restaurante"
          onPress={() =>
            router.push({
              pathname: '/restaurant/new',
              params: {
                latitude: String(mapCenter.latitude),
                longitude: String(mapCenter.longitude),
              },
            })
          }
          style={{
            marginLeft: 'auto',
            minWidth: 44,
            minHeight: 44,
            borderRadius: 14,
            backgroundColor: theme.card,
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <Copy bold size={24}>
            +
          </Copy>
        </Pressable>
      </View>
      <View
        style={{ height: 300, borderRadius: 20, overflow: 'hidden', backgroundColor: theme.line }}
      >
        {Platform.OS === 'ios' ? (
          <AppleMaps.View
            ref={appleMap}
            style={{ flex: 1 }}
            cameraPosition={{ coordinates: initialCenter, zoom: 12 }}
            markers={mapped.map((item) => ({
              id: item.id,
              coordinates: { latitude: item.latitude, longitude: item.longitude },
              title: item.name,
              tintColor: item.glutenSupport === 'UNKNOWN' ? theme.muted : theme.accent,
            }))}
            onMarkerClick={(marker) => {
              const item = items.find((candidate) => candidate.id === marker.id);
              if (item) select(item);
            }}
            onCameraMove={(event) => setMapCenter(event.coordinates as Coordinates)}
          />
        ) : Platform.OS === 'android' && androidMapsConfigured ? (
          <GoogleMaps.View
            ref={googleMap}
            style={{ flex: 1 }}
            cameraPosition={{ coordinates: initialCenter, zoom: 12 }}
            markers={mapped.map((item) => ({
              id: item.id,
              coordinates: { latitude: item.latitude, longitude: item.longitude },
              title: item.name,
            }))}
            onMarkerClick={(marker) => {
              const item = items.find((candidate) => candidate.id === marker.id);
              if (item) select(item);
            }}
            onCameraMove={(event) => setMapCenter(event.coordinates as Coordinates)}
          />
        ) : (
          <View style={{ flex: 1, justifyContent: 'center', padding: 20 }}>
            <Copy>
              El mapa no está disponible en este dispositivo. Puedes consultar la lista de
              restaurantes.
            </Copy>
          </View>
        )}
      </View>
      {selected && (
        <Pressable
          accessibilityRole="button"
          onPress={() => router.push(`/restaurant/${selected.id}`)}
        >
          <Card style={{ borderWidth: 1, borderColor: theme.accent }}>
            <Copy bold>{selected.name} ›</Copy>
            <Copy muted size={13}>
              {selected.address}
            </Copy>
          </Card>
        </Pressable>
      )}
      {search.isFetching && <Copy muted>Buscando restaurantes…</Copy>}
      {search.error && <ErrorNotice error={search.error} retry={() => void search.refetch()} />}
      <Copy muted size={12}>
        {mapAvailable ? `${mapped.length} restaurantes en el mapa · ` : ''}
        {restaurantDisclaimer}
      </Copy>
      <FlatList
        style={{ flex: 1 }}
        data={items}
        keyExtractor={(item) => item.id}
        ListEmptyComponent={
          <Copy muted>
            {search.isSuccess
              ? 'No se han encontrado restaurantes. Prueba otra zona.'
              : 'Busca un restaurante, una ciudad o usa tu ubicación.'}
          </Copy>
        }
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Seleccionar ${item.name}`}
            onPress={() => select(item)}
            style={{ marginBottom: 10 }}
          >
            <Card
              style={{ borderWidth: selectedId === item.id ? 1 : 0, borderColor: theme.accent }}
            >
              <Copy bold>{item.name}</Copy>
              <Copy muted size={13}>
                {item.address}
              </Copy>
              <Copy muted size={12}>
                {item.glutenSupport === 'UNKNOWN'
                  ? '⚠ No verificado'
                  : item.glutenSupport === 'NO_GLUTEN_FREE_OPTIONS'
                    ? 'Sin opciones sin gluten indicadas'
                    : '✓ Opciones sin gluten indicadas'}
              </Copy>
              {selectedId === item.id && (
                <Button
                  title="Ver restaurante"
                  onPress={() => router.push(`/restaurant/${item.id}`)}
                />
              )}
            </Card>
          </Pressable>
        )}
      />
    </Screen>
  );
}
