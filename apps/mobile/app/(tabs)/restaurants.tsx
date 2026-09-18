import { useEffect, useRef, useState } from 'react';
import { FlatList, Platform, Pressable, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { router } from 'expo-router';
import * as Location from 'expo-location';
import { AppleMaps } from 'expo-maps';
import { Camera, Map, Marker, type CameraRef } from '@maplibre/maplibre-react-native';
import { useQuery } from '@tanstack/react-query';
import { z } from 'zod';
import { RestaurantSchema, restaurantDisclaimer, type Restaurant } from '@safeeat/contracts';
import { useDebounced } from '../../src/use-debounced';
import { SearchField } from '../../src/compact';
import { api } from '../../src/session';
import { Button, Card, Copy, ErrorNotice, Screen, Title, useTheme } from '../../src/ui';

type Coordinates = { latitude: number; longitude: number };
type SearchParams = { q: string; latitude?: number; longitude?: number };
const initialCenter: Coordinates = { latitude: 40.4168, longitude: -3.7038 };
const mapStyle = 'https://tiles.openfreemap.org/styles/liberty';
function moved(a: Coordinates, b: Coordinates) {
  return Math.abs(a.latitude - b.latitude) > 0.005 || Math.abs(a.longitude - b.longitude) > 0.007;
}

export default function Restaurants() {
  const theme = useTheme();
  const appleMap = useRef<AppleMaps.MapView>(null);
  const androidCamera = useRef<CameraRef>(null);
  const center = useRef(initialCenter);
  const lastSearch = useRef(initialCenter);
  const moveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [text, setText] = useState('');
  const [editing, setEditing] = useState(false);
  const [params, setParams] = useState<SearchParams>({ q: 'restaurantes', ...initialCenter });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [position, setPosition] = useState<Coordinates | null>(null);
  const [locationError, setLocationError] = useState<unknown>(null);
  const [locating, setLocating] = useState(false);
  const [glutenOnly, setGlutenOnly] = useState(false);
  const settled = useDebounced(text.trim());
  const suggestions = useQuery({
    queryKey: ['restaurant-suggestions', settled],
    enabled: editing && settled.length >= 3,
    queryFn: async ({ signal }) =>
      z
        .object({
          items: z.array(
            z.object({
              label: z.string(),
              kind: z.enum(['restaurant', 'place']),
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
    queryFn: async ({ signal }) => {
      const query = new URLSearchParams({
        q: params.q,
        ...(params.latitude !== undefined
          ? { latitude: String(params.latitude), longitude: String(params.longitude) }
          : {}),
        ...(glutenOnly ? { glutenFree: 'true' } : {}),
      });
      return z
        .object({ items: z.array(RestaurantSchema) })
        .parse(await api(`/restaurants/search?${query}`, { signal }));
    },
    staleTime: 30000,
  });
  const items = search.data?.items ?? [];
  const mapped = items.filter(
    (item): item is Restaurant & Coordinates =>
      item.latitude !== undefined && item.longitude !== undefined,
  );
  const selected = items.find((item) => item.id === selectedId);

  function focus(coordinates: Coordinates, zoom = 15) {
    if (moveTimer.current) clearTimeout(moveTimer.current);
    center.current = coordinates;
    lastSearch.current = coordinates;
    appleMap.current?.setCameraPosition({ coordinates, zoom });
    androidCamera.current?.easeTo({
      center: [coordinates.longitude, coordinates.latitude],
      zoom,
      duration: 300,
    });
  }
  function searchZone(coordinates: Coordinates) {
    center.current = coordinates;
    if (moveTimer.current) clearTimeout(moveTimer.current);
    moveTimer.current = setTimeout(() => {
      if (!moved(coordinates, lastSearch.current)) return;
      lastSearch.current = coordinates;
      setSelectedId(null);
      setParams({ q: 'restaurantes', ...coordinates });
    }, 650);
  }
  useEffect(() => {
    let active = true;
    let watcher: Location.LocationSubscription | undefined;
    void (async () => {
      const permission = await Location.requestForegroundPermissionsAsync();
      if (!permission.granted || !active) return;
      watcher = await Location.watchPositionAsync(
        { accuracy: Location.Accuracy.High, distanceInterval: 10, timeInterval: 5000 },
        (result) => {
          if (active)
            setPosition({
              latitude: result.coords.latitude,
              longitude: result.coords.longitude,
            });
        },
      );
    })().catch(() => {});
    return () => {
      active = false;
      watcher?.remove();
      if (moveTimer.current) clearTimeout(moveTimer.current);
    };
  }, []);

  async function locate() {
    setLocating(true);
    setLocationError(null);
    try {
      const permission = await Location.requestForegroundPermissionsAsync();
      if (!permission.granted)
        throw new Error('Activa la ubicación para centrar el mapa en tu posición.');
      if (Platform.OS === 'android') {
        try {
          await Location.enableNetworkProviderAsync();
        } catch {
          /* Location still works without it. */
        }
      }
      const result = await Location.getCurrentPositionAsync({
        accuracy: Location.Accuracy.Highest,
      });
      const coordinates = { latitude: result.coords.latitude, longitude: result.coords.longitude };
      setPosition(coordinates);
      focus(coordinates, 16);
      setParams({ q: 'restaurantes', ...coordinates });
      if (result.coords.accuracy !== null && result.coords.accuracy > 100)
        setLocationError(
          new Error(
            'Tu ubicación es aproximada. Activa la ubicación precisa en los ajustes del móvil.',
          ),
        );
    } catch (cause) {
      setLocationError(cause);
    } finally {
      setLocating(false);
    }
  }
  function select(item: Restaurant) {
    setSelectedId(item.id);
    if (item.latitude !== undefined && item.longitude !== undefined)
      focus({ latitude: item.latitude, longitude: item.longitude }, 17);
  }
  function submit() {
    const q = text.trim();
    if (q.length < 2) return;
    setEditing(false);
    setParams({ q, ...center.current });
  }
  useEffect(() => {
    if (!search.isSuccess || params.q === 'restaurantes') return;
    const first = search.data.items.find(
      (item) => item.latitude !== undefined && item.longitude !== undefined,
    );
    if (first?.latitude !== undefined && first.longitude !== undefined)
      focus({ latitude: first.latitude, longitude: first.longitude }, 14);
  }, [params.q, search.data, search.isSuccess]);

  return (
    <Screen scroll={false}>
      <Title>Comer fuera</Title>
      <SearchField
        label="Buscar restaurante, calle o lugar"
        value={text}
        onChangeText={(value) => {
          setText(value);
          setEditing(true);
        }}
        placeholder="Restaurante, bar, calle, ciudad…"
        onSubmitEditing={submit}
      />
      {editing && !!suggestions.data?.items.length && (
        <View style={{ backgroundColor: theme.card, borderRadius: 16, overflow: 'hidden' }}>
          {suggestions.data.items.slice(0, 5).map((suggestion, index) => (
            <Pressable
              key={`${suggestion.kind}-${suggestion.label}-${index}`}
              accessibilityRole="button"
              accessibilityLabel={`Buscar ${suggestion.label}`}
              onPress={() => {
                setText(suggestion.label);
                setEditing(false);
                if (suggestion.latitude !== undefined && suggestion.longitude !== undefined) {
                  const coordinates = {
                    latitude: suggestion.latitude,
                    longitude: suggestion.longitude,
                  };
                  focus(coordinates, suggestion.kind === 'place' ? 14 : 16);
                  setParams({ q: suggestion.query, ...coordinates });
                } else setParams({ q: suggestion.query, ...center.current });
              }}
              style={{ padding: 12, minHeight: 48 }}
            >
              <Copy>
                {suggestion.kind === 'place' ? '⌖' : '⌕'} {suggestion.label}
              </Copy>
            </Pressable>
          ))}
        </View>
      )}
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
                latitude: String(center.current.latitude),
                longitude: String(center.current.longitude),
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
        style={{ height: 330, borderRadius: 20, overflow: 'hidden', backgroundColor: theme.line }}
      >
        {Platform.OS === 'ios' ? (
          <AppleMaps.View
            ref={appleMap}
            style={{ flex: 1 }}
            cameraPosition={{ coordinates: initialCenter, zoom: 12 }}
            markers={[
              ...mapped.map((item) => ({
                id: item.id,
                coordinates: { latitude: item.latitude, longitude: item.longitude },
                title: item.name,
                tintColor: item.glutenSupport === 'UNKNOWN' ? theme.muted : theme.accent,
              })),
              ...(position
                ? [
                    {
                      id: 'my-location',
                      coordinates: position,
                      title: 'Tu ubicación',
                      tintColor: '#2588F4',
                    },
                  ]
                : []),
            ]}
            onMarkerClick={(marker) => {
              const item = items.find((candidate) => candidate.id === marker.id);
              if (item) select(item);
            }}
            onCameraMove={(event) => searchZone(event.coordinates as Coordinates)}
          />
        ) : (
          <Map
            mapStyle={mapStyle}
            style={{ flex: 1 }}
            onRegionDidChange={(event) => {
              if (!event.nativeEvent.userInteraction) return;
              const [longitude, latitude] = event.nativeEvent.center;
              searchZone({ latitude, longitude });
            }}
          >
            <Camera
              ref={androidCamera}
              initialViewState={{
                center: [initialCenter.longitude, initialCenter.latitude],
                zoom: 12,
              }}
            />
            {mapped.map((item) => (
              <Marker
                key={item.id}
                id={item.id}
                lngLat={[item.longitude, item.latitude]}
                onPress={() => select(item)}
              >
                <View
                  style={{
                    width: 19,
                    height: 19,
                    borderRadius: 10,
                    borderWidth: 2,
                    borderColor: 'white',
                    backgroundColor: item.glutenSupport === 'UNKNOWN' ? theme.muted : theme.accent,
                  }}
                />
              </Marker>
            ))}
            {position && (
              <Marker id="my-location" lngLat={[position.longitude, position.latitude]}>
                <View
                  style={{
                    width: 19,
                    height: 19,
                    borderRadius: 10,
                    borderWidth: 3,
                    borderColor: 'white',
                    backgroundColor: '#2588F4',
                  }}
                />
              </Marker>
            )}
          </Map>
        )}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Ir a mi ubicación"
          disabled={locating}
          onPress={() => void locate()}
          style={{
            position: 'absolute',
            right: 12,
            bottom: 12,
            width: 48,
            height: 48,
            borderRadius: 15,
            backgroundColor: theme.card,
            alignItems: 'center',
            justifyContent: 'center',
            opacity: locating ? 0.5 : 1,
          }}
        >
          <Ionicons name="locate-outline" size={25} color={theme.accent} />
        </Pressable>
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
        {mapped.length} restaurantes en el mapa · {restaurantDisclaimer}
      </Copy>
      <FlatList
        style={{ flex: 1 }}
        data={items}
        keyExtractor={(item) => item.id}
        ListEmptyComponent={
          <Copy muted>
            {search.isSuccess
              ? 'No se han encontrado restaurantes. Prueba otra zona.'
              : 'Busca un restaurante, una calle, una ciudad o usa tu ubicación.'}
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
