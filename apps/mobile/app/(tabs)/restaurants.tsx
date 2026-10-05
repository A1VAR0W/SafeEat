import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  AppState,
  FlatList,
  Keyboard,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { router, useFocusEffect } from 'expo-router';
import * as Location from 'expo-location';
import { AppleMaps } from 'expo-maps';
import type { CameraRef } from '@maplibre/maplibre-react-native';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { RestaurantSchema, type Restaurant } from '@safeeat/contracts';
import { useDebounced } from '../../src/use-debounced';
import { IconButton, SearchField } from '../../src/compact';
import { api } from '../../src/session';
import { Button, Copy, ErrorNotice, Title, useReducedMotion, useTheme } from '../../src/ui';
import { RestaurantMap, initialCenter } from '../../src/restaurant-map';
import { DirectionsButton, RestaurantBadge } from '../../src/restaurant-components';
import { distanceLabel, hasCoordinates, type Coordinates } from '../../src/restaurant-presentation';

type SearchParams = Coordinates & { q: string };
const emptyItems: Restaurant[] = [];
const filters = [
  ['all', 'Todos'],
  ['options', 'Con opciones'],
  ['dedicated', 'Solo sin gluten'],
] as const;
type GlutenMode = (typeof filters)[number][0];
function moved(a: Coordinates, b: Coordinates) {
  return Math.abs(a.latitude - b.latitude) > 0.002 || Math.abs(a.longitude - b.longitude) > 0.003;
}

export default function Restaurants() {
  const t = useTheme();
  const reduced = useReducedMotion();
  const client = useQueryClient();
  const appleMap = useRef<AppleMaps.MapView>(null);
  const androidCamera = useRef<CameraRef>(null);
  const center = useRef(initialCenter);
  const lastSearch = useRef(initialCenter);
  const ignoreMoveUntil = useRef(0);
  const moveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const didInteract = useRef(false);
  const [text, setText] = useState('');
  const [editing, setEditing] = useState(false);
  const [params, setParams] = useState<SearchParams>({ q: 'restaurantes', ...initialCenter });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [position, setPosition] = useState<Coordinates | null>(null);
  const [locationError, setLocationError] = useState<string | null>(null);
  const [locating, setLocating] = useState(false);
  const [locationGranted, setLocationGranted] = useState(false);
  const [glutenMode, setGlutenMode] = useState<GlutenMode>('all');
  const [resultsOpen, setResultsOpen] = useState(false);
  const [mapError, setMapError] = useState(false);
  const [mapRevision, setMapRevision] = useState(0);
  const [active, setActive] = useState(false);
  const settled = useDebounced(text.trim());
  const suggestLatitude = Math.round(params.latitude * 100) / 100;
  const suggestLongitude = Math.round(params.longitude * 100) / 100;
  const suggestions = useQuery({
    queryKey: ['restaurant-suggestions', settled, suggestLatitude, suggestLongitude],
    enabled: active && editing && settled.length >= 3,
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
        .parse(
          await api(
            `/restaurants/suggest?q=${encodeURIComponent(settled)}&latitude=${suggestLatitude}&longitude=${suggestLongitude}`,
            { signal },
          ),
        ),
    staleTime: 3600000,
    gcTime: 300000,
  });
  const search = useQuery({
    queryKey: ['restaurants', params, glutenMode],
    enabled: active,
    queryFn: async ({ signal }) => {
      const query = new URLSearchParams({
        q: params.q,
        latitude: String(params.latitude),
        longitude: String(params.longitude),
        ...(glutenMode !== 'all' ? { glutenMode } : {}),
      });
      return z
        .object({ items: z.array(RestaurantSchema) })
        .parse(await api(`/restaurants/search?${query}`, { signal }));
    },
    staleTime: 60000,
    gcTime: 300000,
  });
  const items = search.data?.items ?? emptyItems;
  const selected = items.find((item) => item.id === selectedId);
  const mappedCount = useMemo(() => items.filter(hasCoordinates).length, [items]);
  const selectedDistance = selected ? distanceLabel(selected, position) : null;

  const cancelMove = useCallback(() => {
    if (moveTimer.current) clearTimeout(moveTimer.current);
    moveTimer.current = null;
  }, []);
  const focus = useCallback(
    (coordinates: Coordinates, zoom = 15) => {
      cancelMove();
      center.current = coordinates;
      lastSearch.current = coordinates;
      ignoreMoveUntil.current = Date.now() + 650;
      appleMap.current?.setCameraPosition({ coordinates, zoom });
      androidCamera.current?.easeTo({
        center: [coordinates.longitude, coordinates.latitude],
        zoom,
        duration: reduced ? 0 : 250,
      });
    },
    [cancelMove, reduced],
  );
  const searchZone = useCallback(
    (coordinates: Coordinates) => {
      if (
        Date.now() < ignoreMoveUntil.current ||
        !Number.isFinite(coordinates.latitude) ||
        !Number.isFinite(coordinates.longitude)
      )
        return;
      if (!moved(coordinates, center.current)) return;
      didInteract.current = true;
      center.current = coordinates;
      cancelMove();
      moveTimer.current = setTimeout(() => {
        if (!moved(coordinates, lastSearch.current)) return;
        lastSearch.current = coordinates;
        setSelectedId(null);
        setText('');
        setEditing(false);
        setParams({ q: 'restaurantes', ...coordinates });
      }, 650);
    },
    [cancelMove],
  );

  // Tabs stay mounted. GPS and pending searches must stop when this screen is hidden.
  useFocusEffect(
    useCallback(() => {
      let disposed = false;
      let generation = 0;
      let watcher: Location.LocationSubscription | undefined;
      const stop = () => {
        ++generation;
        watcher?.remove();
        watcher = undefined;
      };
      async function start() {
        stop();
        const current = generation;
        const permission = await Location.getForegroundPermissionsAsync();
        if (!disposed && current === generation && !permission.granted && locationGranted)
          setLocationGranted(false);
        if (!permission.granted || disposed || current !== generation) return;
        const subscription = await Location.watchPositionAsync(
          { accuracy: Location.Accuracy.High, distanceInterval: 15, timeInterval: 10000 },
          (result) => {
            if (disposed || current !== generation) return;
            const coordinates = {
              latitude: result.coords.latitude,
              longitude: result.coords.longitude,
            };
            setPosition(coordinates);
            if (!didInteract.current) {
              didInteract.current = true;
              focus(coordinates);
              setParams({ q: 'restaurantes', ...coordinates });
            }
          },
        );
        if (disposed || current !== generation) subscription.remove();
        else watcher = subscription;
      }
      setActive(true);
      void start().catch(() => {});
      const app = AppState.addEventListener('change', (state) => {
        setActive(state === 'active');
        if (state === 'active') void start().catch(() => {});
        else {
          stop();
          cancelMove();
        }
      });
      return () => {
        disposed = true;
        stop();
        app.remove();
        cancelMove();
        setActive(false);
      };
    }, [cancelMove, focus, locationGranted]),
  );

  async function locate() {
    if (locating) return;
    didInteract.current = true;
    cancelMove();
    setLocating(true);
    setLocationError(null);
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      const permission = await Location.requestForegroundPermissionsAsync();
      if (!permission.granted)
        throw new Error('Permite la ubicación en los ajustes del móvil para ver dónde estás.');
      if (Platform.OS === 'android') await Location.enableNetworkProviderAsync().catch(() => {});
      const result = await Promise.race([
        Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.High }),
        new Promise<never>((_, reject) => {
          timeout = setTimeout(
            () =>
              reject(
                new Error(
                  'No hemos podido obtener tu ubicación. Comprueba el GPS y vuelve a intentarlo.',
                ),
              ),
            15000,
          );
        }),
      ]);
      const coordinates = { latitude: result.coords.latitude, longitude: result.coords.longitude };
      setPosition(coordinates);
      focus(coordinates, 16);
      setText('');
      setEditing(false);
      setSelectedId(null);
      Keyboard.dismiss();
      setParams({ q: 'restaurantes', ...coordinates });
      if (result.coords.accuracy !== null && result.coords.accuracy > 100)
        setLocationError(
          'Tu ubicación es aproximada. Activa la ubicación precisa en los ajustes del móvil.',
        );
      setLocationGranted(true);
    } catch (cause) {
      setLocationError(
        cause instanceof Error ? cause.message : 'No se ha podido obtener tu ubicación.',
      );
    } finally {
      if (timeout) clearTimeout(timeout);
      setLocating(false);
    }
  }
  const select = useCallback(
    (item: Restaurant) => {
      didInteract.current = true;
      cancelMove();
      Keyboard.dismiss();
      setEditing(false);
      setResultsOpen(false);
      setSelectedId(item.id);
      if (hasCoordinates(item)) focus(item, 17);
    },
    [cancelMove, focus],
  );
  function submit() {
    const q = text.trim();
    if (q.length < 2) return;
    didInteract.current = true;
    cancelMove();
    Keyboard.dismiss();
    setEditing(false);
    setSelectedId(null);
    setParams({ q, ...center.current });
  }
  const centeredSearch = useRef<unknown>(null);
  useEffect(() => {
    if (
      !active ||
      !search.isSuccess ||
      params.q === 'restaurantes' ||
      centeredSearch.current === search.data
    )
      return;
    centeredSearch.current = search.data;
    const first = search.data.items.find(hasCoordinates);
    if (first) focus(first, 14);
  }, [active, params.q, search.data, search.isSuccess, focus]);
  function openDetails(item: Restaurant) {
    client.setQueryData(['restaurant', item.id], item);
    router.push({ pathname: '/restaurant/[id]', params: { id: item.id } });
  }
  function addRestaurant() {
    setResultsOpen(false);
    router.push({
      pathname: '/restaurant/new',
      params: {
        latitude: String(center.current.latitude),
        longitude: String(center.current.longitude),
      },
    });
  }
  const emptyMessage =
    glutenMode === 'all'
      ? 'No encontramos restaurantes en esta zona.'
      : 'No hay locales con estas opciones indicadas en esta zona.';

  return (
    <SafeAreaView edges={['left', 'right']} style={{ flex: 1, backgroundColor: t.bg }}>
      <View style={{ paddingHorizontal: 14, paddingTop: 8, paddingBottom: 10, gap: 8, zIndex: 2 }}>
        <View style={{ flexDirection: 'row', gap: 8, alignItems: 'center' }}>
          <View style={{ flex: 1 }}>
            <SearchField
              label="Buscar restaurante, calle o lugar"
              value={text}
              maxLength={120}
              onFocus={() => setEditing(true)}
              onChangeText={(value) => {
                setText(value);
                setEditing(true);
              }}
              placeholder="Restaurante, ciudad o calle"
              onSubmitEditing={submit}
            />
          </View>
          {editing && (
            <IconButton
              icon="close"
              label="Cerrar búsqueda"
              onPress={() => {
                setEditing(false);
                Keyboard.dismiss();
              }}
            />
          )}
        </View>
        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={{ gap: 8 }}
        >
          {filters.map(([value, label]) => (
            <Pressable
              key={value}
              accessibilityRole="button"
              accessibilityState={{ selected: glutenMode === value }}
              onPress={() => {
                didInteract.current = true;
                cancelMove();
                setEditing(false);
                Keyboard.dismiss();
                setSelectedId(null);
                setGlutenMode(value);
              }}
              style={({ pressed }) => ({
                minHeight: 44,
                paddingHorizontal: 16,
                paddingVertical: 10,
                borderRadius: 24,
                borderWidth: 1,
                borderColor: glutenMode === value ? t.accent : t.line,
                backgroundColor: glutenMode === value ? t.accent : t.card,
                opacity: pressed ? 0.7 : 1,
              })}
            >
              <Copy size={13} bold color={glutenMode === value ? t.onAccent : t.text}>
                {label}
              </Copy>
            </Pressable>
          ))}
        </ScrollView>
      </View>
      <View style={{ flex: 1, overflow: 'hidden', backgroundColor: t.line }}>
        <RestaurantMap
          key={mapRevision}
          items={items}
          selectedId={selected?.id ?? null}
          position={position}
          dark={t.dark}
          appleMap={appleMap}
          androidCamera={androidCamera}
          onSelect={select}
          onMove={searchZone}
          onError={setMapError}
        />
        {editing && settled.length >= 3 && (
          <View style={[styles.suggestions, styles.shadow, { backgroundColor: t.card }]}>
            <ScrollView keyboardShouldPersistTaps="handled" style={{ maxHeight: 280 }}>
              {suggestions.isFetching && (
                <View style={{ padding: 14 }}>
                  <Copy muted size={13}>
                    Buscando lugares…
                  </Copy>
                </View>
              )}
              {suggestions.data?.items.slice(0, 5).map((suggestion, index) => (
                <Pressable
                  key={`${suggestion.kind}-${index}`}
                  accessibilityRole="button"
                  accessibilityLabel={`Buscar ${suggestion.label}`}
                  onPress={() => {
                    didInteract.current = true;
                    cancelMove();
                    Keyboard.dismiss();
                    setText(suggestion.label);
                    setEditing(false);
                    setSelectedId(null);
                    if (suggestion.latitude !== undefined && suggestion.longitude !== undefined) {
                      const coordinates = {
                        latitude: suggestion.latitude,
                        longitude: suggestion.longitude,
                      };
                      focus(coordinates, suggestion.kind === 'place' ? 14 : 16);
                      setParams({ q: suggestion.query, ...coordinates });
                    } else setParams({ q: suggestion.query, ...center.current });
                  }}
                  style={({ pressed }) => ({
                    flexDirection: 'row',
                    gap: 12,
                    alignItems: 'center',
                    minHeight: 56,
                    padding: 14,
                    borderBottomWidth: StyleSheet.hairlineWidth,
                    borderBottomColor: t.line,
                    opacity: pressed ? 0.6 : 1,
                  })}
                >
                  <Ionicons
                    name={suggestion.kind === 'place' ? 'location-outline' : 'restaurant-outline'}
                    size={21}
                    color={t.accent}
                  />
                  <View style={{ flex: 1 }}>
                    <Copy size={14} numberOfLines={2}>
                      {suggestion.label}
                    </Copy>
                  </View>
                </Pressable>
              ))}
              {!suggestions.isFetching && !suggestions.data?.items.length && (
                <View style={{ padding: 14 }}>
                  <Copy muted size={13}>
                    Pulsa buscar en el teclado para buscar este lugar.
                  </Copy>
                </View>
              )}
            </ScrollView>
          </View>
        )}
        {!editing && (
          <>
            <View style={{ position: 'absolute', top: 12, right: 12, gap: 10 }}>
              <View style={styles.shadow}>
                <IconButton
                  icon={locating ? 'hourglass-outline' : 'locate-outline'}
                  label={locating ? 'Buscando tu ubicación' : 'Ir a mi ubicación'}
                  disabled={locating}
                  onPress={() => void locate()}
                />
              </View>
              <View style={styles.shadow}>
                <IconButton icon="add" label="Añadir restaurante" onPress={addRestaurant} />
              </View>
            </View>
            <View
              pointerEvents="box-none"
              style={{
                position: 'absolute',
                bottom: 16,
                left: 12,
                right: 12,
                maxHeight: '65%',
                gap: 8,
              }}
            >
              {!!locationError && (
                <View style={[styles.notice, { backgroundColor: t.card }]}>
                  <View style={{ flex: 1 }}>
                    <Copy size={13}>{locationError}</Copy>
                  </View>
                  <IconButton
                    icon="close"
                    label="Cerrar aviso de ubicación"
                    onPress={() => setLocationError(null)}
                  />
                </View>
              )}
              {mapError && (
                <View style={[styles.notice, { backgroundColor: t.card }]}>
                  <View style={{ flex: 1 }}>
                    <Copy size={13}>
                      No se ha podido cargar el mapa. Puedes consultar los resultados.
                    </Copy>
                  </View>
                  <IconButton
                    icon="refresh"
                    label="Reintentar mapa"
                    onPress={() => {
                      setMapError(false);
                      setMapRevision((n) => n + 1);
                    }}
                  />
                </View>
              )}
              {selected ? (
                <View style={[styles.preview, styles.shadow, { backgroundColor: t.card }]}>
                  <ScrollView style={{ flexShrink: 1 }} contentContainerStyle={{ gap: 10 }}>
                    <View style={{ flexDirection: 'row', alignItems: 'flex-start', gap: 10 }}>
                      <Pressable
                        accessibilityRole="button"
                        accessibilityLabel={`Ver ficha de ${selected.name}`}
                        onPress={() => openDetails(selected)}
                        style={{ flex: 1, gap: 4 }}
                      >
                        <Copy bold size={21} numberOfLines={2}>
                          {selected.name}
                        </Copy>
                        <Copy muted size={13} numberOfLines={2}>
                          {selected.address || 'Dirección no disponible'}
                          {selectedDistance ? ` · ${selectedDistance} en línea recta` : ''}
                        </Copy>
                      </Pressable>
                      <IconButton
                        icon="close"
                        label="Cerrar restaurante seleccionado"
                        onPress={() => setSelectedId(null)}
                      />
                    </View>
                    <RestaurantBadge restaurant={selected} />
                    <View style={{ flexDirection: 'row', gap: 8 }}>
                      <View style={{ flex: 1 }}>
                        <DirectionsButton restaurant={selected} />
                      </View>
                      <View style={{ flex: 1 }}>
                        <Button secondary title="Ver ficha" onPress={() => openDetails(selected)} />
                      </View>
                    </View>
                  </ScrollView>
                </View>
              ) : (
                <View style={[styles.preview, styles.shadow, { backgroundColor: t.card }]}>
                  <View style={{ flexDirection: 'row', gap: 10, alignItems: 'center' }}>
                    {search.isFetching ? (
                      <ActivityIndicator
                        color={t.accent}
                        accessibilityLabel="Buscando restaurantes"
                      />
                    ) : (
                      <Ionicons
                        name={items.length ? 'restaurant-outline' : 'search-outline'}
                        color={t.accent}
                        size={24}
                      />
                    )}
                    <View style={{ flex: 1, gap: 3 }}>
                      <Copy bold size={16}>
                        {search.isFetching
                          ? 'Buscando en esta zona…'
                          : search.isError
                            ? 'No se han podido cargar los lugares'
                            : items.length
                              ? `${items.length}${items.length >= 100 ? '+' : ''} lugares para descubrir`
                              : emptyMessage}
                      </Copy>
                      <Copy muted size={12}>
                        {search.isError
                          ? 'Comprueba tu conexión y vuelve a intentarlo.'
                          : items.length
                            ? 'Toca un marcador o abre los resultados'
                            : 'Mueve el mapa o prueba otra búsqueda.'}
                      </Copy>
                    </View>
                    {search.isError && (
                      <IconButton
                        icon="refresh"
                        label="Reintentar búsqueda"
                        onPress={() => void search.refetch()}
                      />
                    )}
                  </View>
                </View>
              )}
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={`Ver resultados, ${items.length} restaurantes`}
                onPress={() => {
                  cancelMove();
                  setResultsOpen(true);
                }}
                style={({ pressed }) => [
                  styles.resultsButton,
                  styles.shadow,
                  { backgroundColor: t.card, opacity: pressed ? 0.7 : 1 },
                ]}
              >
                <Ionicons name="list-outline" size={20} color={t.text} />
                <Copy bold size={14}>
                  Ver resultados{items.length ? ` · ${items.length}` : ''}
                </Copy>
                {selected && search.isFetching && (
                  <ActivityIndicator size="small" color={t.accent} />
                )}
              </Pressable>
            </View>
          </>
        )}
      </View>
      <Modal
        visible={resultsOpen}
        animationType={reduced ? 'none' : 'slide'}
        onRequestClose={() => setResultsOpen(false)}
        presentationStyle="fullScreen"
      >
        <SafeAreaView style={{ flex: 1, backgroundColor: t.bg }}>
          <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center', padding: 16 }}>
            <View style={{ flex: 1 }}>
              <Title>Dónde comer</Title>
              <Copy muted size={13}>
                {items.length} resultados · {filters.find(([value]) => value === glutenMode)?.[1]}
              </Copy>
            </View>
            <IconButton
              icon="map-outline"
              label="Volver al mapa"
              onPress={() => setResultsOpen(false)}
            />
          </View>
          <FlatList
            data={items}
            keyExtractor={(item) => item.id}
            initialNumToRender={8}
            maxToRenderPerBatch={8}
            windowSize={5}
            contentContainerStyle={{ padding: 16, paddingTop: 0, gap: 10, paddingBottom: 32 }}
            ListHeaderComponent={
              <View style={{ gap: 12, paddingBottom: 12 }}>
                <Copy muted size={13}>
                  Elige un restaurante para verlo en el mapa. Confirma las opciones sin gluten con
                  el personal.
                </Copy>
                {search.isFetching && <ActivityIndicator color={t.accent} />}
                {search.error && (
                  <ErrorNotice error={search.error} retry={() => void search.refetch()} />
                )}
                {mappedCount < items.length && (
                  <Copy muted size={12}>
                    Algunos lugares aún no tienen ubicación en el mapa.
                  </Copy>
                )}
              </View>
            }
            ListEmptyComponent={
              <View style={{ gap: 16, paddingVertical: 32 }}>
                <Copy bold>{search.isFetching ? 'Buscando restaurantes…' : emptyMessage}</Copy>
                <Button title="Volver al mapa" onPress={() => setResultsOpen(false)} />
                <Button secondary title="Añadir un restaurante" onPress={addRestaurant} />
              </View>
            }
            renderItem={({ item }) => (
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={`Seleccionar ${item.name}`}
                onPress={() => select(item)}
                style={({ pressed }) => ({
                  flexDirection: 'row',
                  alignItems: 'center',
                  gap: 12,
                  padding: 16,
                  borderRadius: 20,
                  backgroundColor: t.card,
                  opacity: pressed ? 0.7 : 1,
                })}
              >
                <View
                  style={{
                    height: 44,
                    width: 44,
                    alignItems: 'center',
                    justifyContent: 'center',
                    borderRadius: 15,
                    backgroundColor: t.bg,
                  }}
                >
                  <Ionicons name="restaurant-outline" size={23} color={t.accent} />
                </View>
                <View style={{ flex: 1, gap: 5 }}>
                  <Copy bold>{item.name}</Copy>
                  <Copy muted size={12} numberOfLines={2}>
                    {item.address || 'Dirección no disponible'}
                  </Copy>
                  <RestaurantBadge restaurant={item} />
                  {distanceLabel(item, position) && (
                    <Copy muted size={12}>
                      {distanceLabel(item, position)} en línea recta
                    </Copy>
                  )}
                </View>
                <Ionicons name="chevron-forward" size={19} color={t.muted} />
              </Pressable>
            )}
          />
        </SafeAreaView>
      </Modal>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  shadow: {
    shadowColor: '#000',
    shadowOpacity: 0.13,
    shadowOffset: { width: 0, height: 3 },
    shadowRadius: 10,
    elevation: 4,
  },
  suggestions: {
    position: 'absolute',
    top: 8,
    left: 12,
    right: 12,
    borderRadius: 18,
    overflow: 'hidden',
    zIndex: 5,
  },
  notice: { flexDirection: 'row', alignItems: 'center', gap: 6, padding: 12, borderRadius: 18 },
  preview: { padding: 16, borderRadius: 24, flexShrink: 1 },
  resultsButton: {
    alignSelf: 'center',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    minHeight: 48,
    paddingHorizontal: 20,
    paddingVertical: 12,
    borderRadius: 28,
  },
});
