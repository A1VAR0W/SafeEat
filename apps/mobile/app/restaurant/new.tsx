import { useRef, useState } from 'react';
import { Platform, View } from 'react-native';
import { router, useLocalSearchParams } from 'expo-router';
import { AppleMaps } from 'expo-maps';
import { Camera, Map, type CameraRef } from '@maplibre/maplibre-react-native';
import * as Location from 'expo-location';
import { useQueryClient } from '@tanstack/react-query';
import { RestaurantSchema } from '@safeeat/contracts';
import { RestaurantSupportPicker, type RestaurantSupport } from '../../src/restaurant-support';
import { api, useSession } from '../../src/session';
import { Button, Copy, ErrorNotice, Field, Screen, Title, useTheme } from '../../src/ui';

export default function NewRestaurant() {
  const params = useLocalSearchParams<{ latitude?: string; longitude?: string }>();
  const initial = {
    latitude: Number.isFinite(Number(params.latitude)) ? Number(params.latitude) : 40.4168,
    longitude: Number.isFinite(Number(params.longitude)) ? Number(params.longitude) : -3.7038,
  };
  const { user } = useSession();
  const client = useQueryClient();
  const t = useTheme();
  const appleMap = useRef<AppleMaps.MapView>(null);
  const androidCamera = useRef<CameraRef>(null);
  const [position, setPosition] = useState(initial);
  const [name, setName] = useState('');
  const [address, setAddress] = useState('');
  const [support, setSupport] = useState<RestaurantSupport | null>(null);
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function locate() {
    try {
      const permission = await Location.requestForegroundPermissionsAsync();
      if (!permission.granted) throw new Error('No se ha concedido acceso a la ubicación.');
      const result = await Location.getCurrentPositionAsync({
        accuracy: Location.Accuracy.Highest,
      });
      const coordinates = { latitude: result.coords.latitude, longitude: result.coords.longitude };
      setPosition(coordinates);
      appleMap.current?.setCameraPosition({ coordinates, zoom: 17 });
      androidCamera.current?.easeTo({
        center: [coordinates.longitude, coordinates.latitude],
        zoom: 17,
        duration: 300,
      });
    } catch (cause) {
      setError(cause);
    }
  }
  async function create() {
    if (
      !support ||
      name.trim().length < 2 ||
      address.trim().length < 5 ||
      note.trim().length < 10 ||
      busy
    )
      return;
    if (!user) return router.push('/auth/login');
    setBusy(true);
    setError(null);
    try {
      const item = RestaurantSchema.parse(
        await api(
          '/restaurants',
          {
            method: 'POST',
            body: JSON.stringify({
              name: name.trim(),
              address: address.trim(),
              ...position,
              support,
              note: note.trim(),
            }),
          },
          true,
        ),
      );
      await client.invalidateQueries({ queryKey: ['restaurants'] });
      router.replace(`/restaurant/${item.id}`);
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Title>Añadir restaurante</Title>
      <Copy muted>Mueve el mapa hasta colocar el centro sobre la entrada del restaurante.</Copy>
      <View style={{ height: 220, borderRadius: 18, overflow: 'hidden', backgroundColor: t.line }}>
        {Platform.OS === 'ios' ? (
          <AppleMaps.View
            ref={appleMap}
            style={{ flex: 1 }}
            cameraPosition={{ coordinates: initial, zoom: 16 }}
            onCameraMove={(event) => setPosition(event.coordinates as typeof position)}
          />
        ) : Platform.OS === 'android' ? (
          <Map
            mapStyle="https://tiles.openfreemap.org/styles/liberty"
            style={{ flex: 1 }}
            onRegionDidChange={(event) => {
              const [longitude, latitude] = event.nativeEvent.center;
              setPosition({ latitude, longitude });
            }}
          >
            <Camera
              ref={androidCamera}
              initialViewState={{ center: [initial.longitude, initial.latitude], zoom: 16 }}
            />
          </Map>
        ) : (
          <View style={{ flex: 1, justifyContent: 'center', padding: 16 }}>
            <Copy>Usa tu ubicación para situar el restaurante.</Copy>
          </View>
        )}
        {(Platform.OS === 'ios' || Platform.OS === 'android') && (
          <View
            pointerEvents="none"
            style={{ position: 'absolute', left: 0, right: 0, top: 92, alignItems: 'center' }}
          >
            <Copy bold size={30}>
              ＋
            </Copy>
          </View>
        )}
      </View>
      <Button secondary title="Usar mi ubicación" onPress={() => void locate()} />
      <Field label="Nombre del restaurante" value={name} onChangeText={setName} maxLength={160} />
      <Field label="Dirección" value={address} onChangeText={setAddress} maxLength={300} />
      <Copy bold>Opciones sin gluten indicadas</Copy>
      <RestaurantSupportPicker value={support} onChange={setSupport} />
      <Field
        label="¿Qué has comprobado?"
        value={note}
        onChangeText={setNote}
        multiline
        maxLength={500}
        placeholder="Explica lo que te dijo el restaurante sobre el menú y la preparación…"
      />
      <Copy muted size={12}>
        Las aportaciones de usuarios se muestran como tales. Confirma siempre ingredientes y
        contaminación cruzada con el restaurante.
      </Copy>
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Añadiendo…' : 'Añadir restaurante'}
        disabled={
          busy ||
          !support ||
          name.trim().length < 2 ||
          address.trim().length < 5 ||
          note.trim().length < 10
        }
        onPress={() => void create()}
      />
    </Screen>
  );
}
