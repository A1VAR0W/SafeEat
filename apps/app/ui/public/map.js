// Thin MapLibre GL JS glue for the Leptos UI. MapLibre (~940 kB) is loaded only
// when the Restaurants tab first opens, so the scanner starts as fast as possible.
/* global maplibregl -- defined by vendor/maplibre-gl.js once it has loaded */
'use strict';
(function () {
  let loading = null;
  function load() {
    if (window.maplibregl) return Promise.resolve();
    if (loading) return loading;
    loading = new Promise(function (resolve, reject) {
      const css = document.createElement('link');
      css.rel = 'stylesheet';
      css.href = 'vendor/maplibre-gl.css';
      document.head.appendChild(css);
      const js = document.createElement('script');
      js.src = 'vendor/maplibre-gl.js';
      js.onload = function () {
        resolve();
      };
      js.onerror = function () {
        loading = null;
        reject(new Error('map'));
      };
      document.head.appendChild(js);
    });
    return loading;
  }

  // Same meaning as the list badges: favourable information is orientative (blue),
  // never a green "safe"; warnings stay red.
  const COLORS = {
    DEDICATED_GLUTEN_FREE: '#1F4F8A',
    GLUTEN_FREE_OPTIONS: '#4A7FC1',
    NO_GLUTEN_FREE_OPTIONS: '#A02D22',
    UNKNOWN: '#857A5C',
  };

  let map = null;
  let ready = false;
  let pending = null;
  let handlers = {};
  // A camera asked for before MapLibre finished loading (the location often arrives
  // first): used as the starting view instead of being lost.
  let camera = null;
  let selected = '';

  function setData(fc) {
    if (!ready) {
      pending = fc;
      return;
    }
    map.getSource('places').setData(fc);
  }

  window.seMap = {
    // Creates (or re-attaches) the map. `onMove(lat,lng,zoom)` fires after the user
    // pans, `onSelect(id)` when a marker is tapped.
    mount: function (el, lat, lng, zoom, onMove, onSelect, onError) {
      handlers = { onMove: onMove, onSelect: onSelect };
      load()
        .then(function () {
          if (map) {
            el.appendChild(map.getContainer());
            map.resize();
            return;
          }
          const container = document.createElement('div');
          container.className = 'map-canvas';
          el.appendChild(container);
          map = new maplibregl.Map({
            container: container,
            style: 'https://tiles.openfreemap.org/styles/liberty',
            center: camera ? [camera.lng, camera.lat] : [lng, lat],
            zoom: camera ? camera.zoom : zoom,
            attributionControl: { compact: true },
            fadeDuration: 0,
            pitchWithRotate: false,
            dragRotate: false,
            maxPitch: 0,
          });
          camera = null;
          map.touchZoomRotate.disableRotation();
          map.on('load', function () {
            map.addSource('places', {
              type: 'geojson',
              data: { type: 'FeatureCollection', features: [] },
              cluster: true,
              clusterRadius: 44,
              clusterMaxZoom: 15,
            });
            map.addLayer({
              id: 'clusters',
              type: 'circle',
              source: 'places',
              filter: ['has', 'point_count'],
              paint: {
                'circle-color': '#064B39',
                'circle-opacity': 0.88,
                'circle-radius': ['step', ['get', 'point_count'], 16, 10, 20, 50, 26],
                'circle-stroke-width': 2,
                'circle-stroke-color': '#FFFFFF',
              },
            });
            map.addLayer({
              id: 'cluster-count',
              type: 'symbol',
              source: 'places',
              filter: ['has', 'point_count'],
              layout: {
                'text-field': ['get', 'point_count_abbreviated'],
                'text-size': 13,
                'text-font': ['Noto Sans Bold'],
              },
              paint: { 'text-color': '#FFFFFF' },
            });
            map.addLayer({
              id: 'place',
              type: 'circle',
              source: 'places',
              filter: ['!', ['has', 'point_count']],
              paint: {
                'circle-color': [
                  'match',
                  ['get', 'support'],
                  'DEDICATED_GLUTEN_FREE',
                  COLORS.DEDICATED_GLUTEN_FREE,
                  'GLUTEN_FREE_OPTIONS',
                  COLORS.GLUTEN_FREE_OPTIONS,
                  'NO_GLUTEN_FREE_OPTIONS',
                  COLORS.NO_GLUTEN_FREE_OPTIONS,
                  COLORS.UNKNOWN,
                ],
                'circle-radius': ['case', ['get', 'highlight'], 10, 8],
                'circle-stroke-width': ['case', ['get', 'highlight'], 3, 2],
                'circle-stroke-color': '#FFFFFF',
              },
            });
            // The tapped place, drawn on top with a ring.
            map.addLayer({
              id: 'place-selected',
              type: 'circle',
              source: 'places',
              filter: ['==', ['get', 'id'], selected],
              paint: {
                'circle-color': 'rgba(0,0,0,0)',
                'circle-radius': 15,
                'circle-stroke-width': 3,
                'circle-stroke-color': '#0E1A16',
              },
            });
            map.on('click', 'clusters', function (e) {
              const f = e.features[0];
              const tap = e.originalEvent;
              map
                .getSource('places')
                .getClusterExpansionZoom(f.properties.cluster_id)
                .then(function (z) {
                  // Her tap moved the map: the list follows, as with a pan.
                  map.easeTo(
                    { center: f.geometry.coordinates, zoom: z, duration: 250 },
                    { originalEvent: tap },
                  );
                });
            });
            map.on('click', 'place', function (e) {
              if (handlers.onSelect) handlers.onSelect(String(e.features[0].properties.id));
            });
            ready = true;
            if (pending) {
              map.getSource('places').setData(pending);
              pending = null;
            }
            // Only report position after the style exists, so the first search uses
            // the real camera and not a default city.
            const c = map.getCenter();
            if (handlers.onMove) handlers.onMove(c.lat, c.lng, map.getZoom(), false);
          });
          map.on('moveend', function (e) {
            if (!ready || !e.originalEvent) return; // ignore programmatic moves
            const c = map.getCenter();
            if (handlers.onMove) handlers.onMove(c.lat, c.lng, map.getZoom(), true);
          });
        })
        .catch(function () {
          if (onError) onError();
        });
    },
    setPlaces: function (json) {
      setData(JSON.parse(json));
    },
    flyTo: function (lat, lng, zoom) {
      if (map) map.jumpTo({ center: [lng, lat], zoom: zoom || map.getZoom() });
      else camera = { lat: lat, lng: lng, zoom: zoom || 14 };
    },
    // Shows every result of a typed search (they may lie outside the current view).
    fitPlaces: function (json) {
      const fc = JSON.parse(json);
      if (!map || !fc.features.length) return;
      const b = new maplibregl.LngLatBounds();
      fc.features.forEach(function (f) {
        b.extend(f.geometry.coordinates);
      });
      map.fitBounds(b, { padding: 64, maxZoom: 15, duration: 0 });
    },
    select: function (id) {
      selected = id || '';
      if (ready) map.setFilter('place-selected', ['==', ['get', 'id'], selected]);
    },
    center: function () {
      if (!map) return null;
      const c = map.getCenter();
      return [c.lat, c.lng];
    },
  };
})();
