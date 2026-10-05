import { memo, useMemo, useRef, type RefObject } from 'react';
import { Platform, StyleSheet } from 'react-native';
import { AppleMaps } from 'expo-maps';
import {
  Camera,
  GeoJSONSource,
  Layer,
  Map,
  type CameraRef,
  type GeoJSONSourceRef,
} from '@maplibre/maplibre-react-native';
import type { Restaurant } from '@safeeat/contracts';
import {
  hasCoordinates,
  restaurantFeatures,
  restaurantStatus,
  type Coordinates,
} from './restaurant-presentation';

export const initialCenter = { latitude: 40.4168, longitude: -3.7038 };
const initialView = {
  center: [initialCenter.longitude, initialCenter.latitude] as [number, number],
  zoom: 14,
};
const appleCamera = { coordinates: initialCenter, zoom: 14 };

export const RestaurantMap = memo(function RestaurantMap({
  items,
  selectedId,
  position,
  dark,
  appleMap,
  androidCamera,
  onSelect,
  onMove,
  onError,
  onMapPress,
}: {
  items: Restaurant[];
  selectedId: string | null;
  position: Coordinates | null;
  dark: boolean;
  appleMap: RefObject<AppleMaps.MapView | null>;
  androidCamera: RefObject<CameraRef | null>;
  onSelect: (item: Restaurant) => void;
  onMove: (coordinates: Coordinates) => void;
  onError: (failed: boolean) => void;
  onMapPress: () => void;
}) {
  const source = useRef<GeoJSONSourceRef>(null);
  const features = useMemo(() => restaurantFeatures(items), [items]);
  const markers = useMemo(
    () =>
      items.filter(hasCoordinates).map((item) => ({
        id: item.id,
        coordinates: { latitude: item.latitude, longitude: item.longitude },
        title: item.name,
        systemImage: item.glutenSupport === 'DEDICATED_GLUTEN_FREE' ? 'leaf.fill' : 'fork.knife',
        tintColor: selectedId === item.id ? '#124BC0' : restaurantStatus(item.glutenSupport).color,
      })),
    [items, selectedId],
  );
  const userPoint = useMemo<GeoJSON.FeatureCollection<GeoJSON.Point>>(
    () => ({
      type: 'FeatureCollection',
      features: position
        ? [
            {
              type: 'Feature',
              properties: {},
              geometry: { type: 'Point', coordinates: [position.longitude, position.latitude] },
            },
          ]
        : [],
    }),
    [position],
  );

  if (Platform.OS === 'ios')
    return (
      <AppleMaps.View
        ref={appleMap}
        style={StyleSheet.absoluteFill}
        cameraPosition={appleCamera}
        colorScheme={dark ? AppleMaps.MapColorScheme.DARK : AppleMaps.MapColorScheme.LIGHT}
        properties={{ isMyLocationEnabled: !!position, selectionEnabled: false }}
        uiSettings={{ myLocationButtonEnabled: false, togglePitchEnabled: false }}
        markers={markers}
        onMapClick={onMapPress}
        onMarkerClick={(marker) => {
          const item = items.find((r) => r.id === marker.id);
          if (item) onSelect(item);
        }}
        onCameraMove={(event) => {
          if (
            event.coordinates?.latitude !== undefined &&
            event.coordinates.longitude !== undefined
          )
            onMove(event.coordinates as Coordinates);
        }}
      />
    );

  return (
    <Map
      style={StyleSheet.absoluteFill}
      mapStyle={`https://tiles.openfreemap.org/styles/${dark ? 'dark' : 'bright'}`}
      // Texture rendering lets native controls and cards layer above the map reliably.
      androidView="texture"
      attributionPosition={{ left: 8, top: 8 }}
      logo={false}
      compassPosition={{ right: 12, top: 126 }}
      onDidFinishLoadingMap={() => onError(false)}
      onDidFailLoadingMap={() => onError(true)}
      onPress={onMapPress}
      onRegionDidChange={(event) => {
        if (!event.nativeEvent.userInteraction) return;
        const [longitude, latitude] = event.nativeEvent.center;
        onMove({ latitude, longitude });
      }}
    >
      <Camera ref={androidCamera} initialViewState={initialView} />
      <GeoJSONSource
        ref={source}
        id="restaurants"
        data={features}
        cluster
        clusterRadius={42}
        clusterMaxZoom={15}
        onPress={(event) => {
          event.stopPropagation();
          const feature = event.nativeEvent.features[0];
          if (!feature || feature.geometry.type !== 'Point') return;
          if (feature.properties?.cluster) {
            const coordinates = feature.geometry.coordinates as [number, number];
            void source.current
              ?.getClusterExpansionZoom(Number(feature.properties.cluster_id))
              .then((zoom) => {
                androidCamera.current?.easeTo({
                  center: coordinates,
                  zoom: Math.min(zoom + 0.5, 18),
                  duration: 250,
                });
              })
              .catch(() => {
                androidCamera.current?.easeTo({ center: coordinates, zoom: 16, duration: 250 });
              });
          } else {
            const item = items.find((r) => r.id === feature.properties?.id);
            if (item) onSelect(item);
          }
        }}
      >
        <Layer
          id="restaurant-clusters"
          type="circle"
          filter={['has', 'point_count']}
          paint={{
            'circle-color': '#087653',
            'circle-radius': 20,
            'circle-stroke-width': 3,
            'circle-stroke-color': '#FFFFFF',
          }}
        />
        <Layer
          id="restaurant-counts"
          type="symbol"
          filter={['has', 'point_count']}
          layout={{
            'text-field': ['get', 'point_count_abbreviated'],
            'text-size': 13,
            'text-font': ['Noto Sans Regular'],
            'text-allow-overlap': true,
          }}
          paint={{ 'text-color': '#FFFFFF' }}
        />
        <Layer
          id="restaurant-points"
          type="circle"
          filter={['!', ['has', 'point_count']]}
          paint={{
            'circle-color': [
              'case',
              ['==', ['get', 'id'], selectedId ?? ''],
              '#124BC0',
              ['get', 'color'],
            ],
            'circle-radius': ['case', ['==', ['get', 'id'], selectedId ?? ''], 15, 11],
            'circle-stroke-width': 3,
            'circle-stroke-color': '#FFFFFF',
          }}
        />
        <Layer
          id="restaurant-labels"
          type="symbol"
          minzoom={16}
          filter={['!', ['has', 'point_count']]}
          layout={{
            'text-field': ['get', 'name'],
            'text-size': 12,
            'text-font': ['Noto Sans Regular'],
            'text-anchor': 'top',
            'text-offset': [0, 1.5],
            'text-max-width': 10,
          }}
          paint={{
            'text-color': dark ? '#F0F8F0' : '#19362B',
            'text-halo-color': dark ? '#152720' : '#FFFFFF',
            'text-halo-width': 2,
          }}
        />
      </GeoJSONSource>
      <GeoJSONSource id="user-location" data={userPoint}>
        <Layer
          id="location-halo"
          type="circle"
          paint={{ 'circle-color': '#2588F4', 'circle-opacity': 0.15, 'circle-radius': 22 }}
        />
        <Layer
          id="location-dot"
          type="circle"
          paint={{
            'circle-color': '#2588F4',
            'circle-radius': 7,
            'circle-stroke-width': 3,
            'circle-stroke-color': '#FFFFFF',
          }}
        />
      </GeoJSONSource>
    </Map>
  );
});
