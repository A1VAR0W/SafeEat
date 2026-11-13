// Thin MapLibre GL JS glue for the Leptos UI. MapLibre (~940 kB) is loaded only
// when the Restaurants tab first opens, so the scanner starts as fast as possible.
//
// The map always looks complete: the app loads every restaurant of an area much
// wider than the screen (`addArea`) and the map keeps what it has, so panning only
// adds pins, never empties the view. The filter is applied here, instantly. A typed
// search shows only its results (`setResults` + `useResults(true)`).
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
    GLUTEN_FREE_OPTIONS: '#3F78BF',
    NO_GLUTEN_FREE_OPTIONS: '#B3261E',
    UNKNOWN: '#857A5C',
  };
  const FAVOURABLE = { DEDICATED_GLUTEN_FREE: true, GLUTEN_FREE_OPTIONS: true };
  // Pins kept in memory; beyond this the farthest from the view are dropped.
  const KEEP_MAX = 9000;
  const KEEP_AFTER_TRIM = 7000;

  let map = null;
  let ready = false;
  let handlers = {};
  // A camera asked for before MapLibre finished loading (the location often arrives
  // first): used as the starting view instead of being lost.
  let camera = null;
  let selected = '';
  const area = new Map(); // id → {id,name,lat,lng,support}
  let results = [];
  let showResults = false;
  let filter = 'all';
  let frame = 0;

  // ---------- pins ----------

  // A teardrop pin with a fork and knife, drawn once per colour (2× for sharp edges).
  function pinImage(color) {
    const W = 64;
    const H = 84;
    const c = document.createElement('canvas');
    c.width = W;
    c.height = H;
    const g = c.getContext('2d');
    const cx = W / 2;
    const cy = 28;
    const R = 22;
    const tip = 76;
    g.shadowColor = 'rgba(0,0,0,0.35)';
    g.shadowBlur = 5;
    g.shadowOffsetY = 2;
    g.beginPath();
    g.moveTo(cx, tip);
    g.arc(cx, cy, R, Math.PI / 6, (5 * Math.PI) / 6, true);
    g.closePath();
    g.fillStyle = color;
    g.fill();
    g.shadowColor = 'transparent';
    g.lineWidth = 3;
    g.strokeStyle = '#FFFFFF';
    g.stroke();
    // Fork.
    g.fillStyle = '#FFFFFF';
    g.lineCap = 'round';
    g.lineWidth = 2.4;
    [-10, -6, -2].forEach(function (dx) {
      g.beginPath();
      g.moveTo(cx + dx, cy - 12);
      g.lineTo(cx + dx, cy - 4);
      g.stroke();
    });
    g.beginPath();
    g.moveTo(cx - 10, cy - 4);
    g.quadraticCurveTo(cx - 6, cy + 1, cx - 2, cy - 4);
    g.stroke();
    g.lineWidth = 3.4;
    g.beginPath();
    g.moveTo(cx - 6, cy - 1);
    g.lineTo(cx - 6, cy + 13);
    g.stroke();
    // Knife.
    g.beginPath();
    g.ellipse(cx + 7, cy - 5, 3.6, 8, 0, 0, Math.PI * 2);
    g.fill();
    g.beginPath();
    g.moveTo(cx + 7, cy + 1);
    g.lineTo(cx + 7, cy + 13);
    g.stroke();
    return g.getImageData(0, 0, W, H);
  }

  function addImages() {
    Object.keys(COLORS).forEach(function (k) {
      map.addImage('pin-' + k, pinImage(COLORS[k]), { pixelRatio: 2 });
    });
  }

  // ---------- data ----------

  function passes(p) {
    if (filter === 'options') return !!FAVOURABLE[p.support];
    if (filter === 'dedicated') return p.support === 'DEDICATED_GLUTEN_FREE';
    return true;
  }

  function feature(p) {
    const support = COLORS[p.support] ? p.support : 'UNKNOWN';
    return {
      type: 'Feature',
      geometry: { type: 'Point', coordinates: [p.lng, p.lat] },
      properties: {
        id: p.id,
        name: p.name,
        support: support,
        icon: 'pin-' + support,
      },
    };
  }

  function find(id) {
    if (!id) return null;
    for (let i = 0; i < results.length; i++) if (results[i].id === id) return results[i];
    return area.get(id) || null;
  }

  function render() {
    frame = 0;
    if (!ready) return;
    const list = showResults ? results : Array.from(area.values());
    const good = [];
    const rest = [];
    list.forEach(function (p) {
      if (!passes(p)) return;
      (FAVOURABLE[p.support] ? good : rest).push(feature(p));
    });
    map.getSource('good').setData({ type: 'FeatureCollection', features: good });
    map.getSource('rest').setData({ type: 'FeatureCollection', features: rest });
    const s = find(selected);
    map.getSource('sel').setData({ type: 'FeatureCollection', features: s ? [feature(s)] : [] });
  }

  // One redraw per frame, however many updates arrive.
  function schedule() {
    if (!frame) frame = requestAnimationFrame(render);
  }

  function trim() {
    if (area.size <= KEEP_MAX || !map) return;
    const c = map.getCenter();
    const far = Array.from(area.values())
      .map(function (p) {
        const dx = (p.lng - c.lng) * Math.cos((c.lat * Math.PI) / 180);
        const dy = p.lat - c.lat;
        return [dx * dx + dy * dy, p.id];
      })
      .sort(function (a, b) {
        return b[0] - a[0];
      });
    for (let i = 0; i < far.length && area.size > KEEP_AFTER_TRIM; i++) {
      if (far[i][1] !== selected) area.delete(far[i][1]);
    }
  }

  // ---------- map ----------

  function report() {
    if (!handlers.onView) return;
    const b = map.getBounds();
    handlers.onView(b.getSouth(), b.getWest(), b.getNorth(), b.getEast(), map.getZoom());
  }

  function addLayers() {
    const empty = { type: 'FeatureCollection', features: [] };
    map.addSource('rest', {
      type: 'geojson',
      data: empty,
      cluster: true,
      clusterRadius: 52,
      clusterMaxZoom: 13,
    });
    // Places with gluten-free options are never hidden inside a cluster.
    map.addSource('good', { type: 'geojson', data: empty });
    map.addSource('sel', { type: 'geojson', data: empty });

    map.addLayer({
      id: 'clusters',
      type: 'circle',
      source: 'rest',
      filter: ['has', 'point_count'],
      paint: {
        'circle-color': '#FFFFFF',
        'circle-radius': ['step', ['get', 'point_count'], 12, 10, 15, 50, 18, 200, 22],
        'circle-opacity': 0.92,
        'circle-stroke-width': 2,
        'circle-stroke-color': '#857A5C',
      },
    });
    map.addLayer({
      id: 'cluster-count',
      type: 'symbol',
      source: 'rest',
      filter: ['has', 'point_count'],
      layout: {
        'text-field': ['get', 'point_count_abbreviated'],
        'text-size': 12,
        'text-font': ['Noto Sans Bold'],
        'text-allow-overlap': true,
      },
      paint: { 'text-color': '#3D3826' },
    });
    const label = function (fromZoom) {
      return {
        'text-field': ['step', ['zoom'], '', fromZoom, ['get', 'name']],
        'text-font': ['Noto Sans Regular'],
        'text-size': 12,
        'text-anchor': 'top',
        'text-offset': [0, 0.25],
        'text-max-width': 9,
        'text-optional': true,
      };
    };
    const labelPaint = {
      'text-color': '#1D2A25',
      'text-halo-color': '#FFFFFF',
      'text-halo-width': 1.6,
    };
    const unknown = ['==', ['get', 'support'], 'UNKNOWN'];
    map.addLayer({
      id: 'rest-pins',
      type: 'symbol',
      source: 'rest',
      filter: ['!', ['has', 'point_count']],
      layout: Object.assign(
        {
          'icon-image': ['get', 'icon'],
          'icon-anchor': 'bottom',
          'icon-allow-overlap': true,
          // Places without data: small pins, like minor places on Google Maps.
          'icon-size': ['case', unknown, 0.62, 0.85],
        },
        label(16),
      ),
      paint: labelPaint,
    });
    map.addLayer({
      id: 'good-pins',
      type: 'symbol',
      source: 'good',
      layout: Object.assign(
        {
          'icon-image': ['get', 'icon'],
          'icon-anchor': 'bottom',
          'icon-allow-overlap': true,
          'symbol-sort-key': ['case', ['==', ['get', 'support'], 'DEDICATED_GLUTEN_FREE'], 1, 0],
        },
        label(14),
      ),
      paint: Object.assign({}, labelPaint, { 'text-color': '#16365F' }),
    });
    // The tapped place, larger and on top, always with its name.
    map.addLayer({
      id: 'sel-pin',
      type: 'symbol',
      source: 'sel',
      layout: {
        'icon-image': ['get', 'icon'],
        'icon-anchor': 'bottom',
        'icon-allow-overlap': true,
        'icon-size': 1.3,
        'text-field': ['get', 'name'],
        'text-font': ['Noto Sans Bold'],
        'text-size': 13,
        'text-anchor': 'top',
        'text-offset': [0, 0.35],
        'text-max-width': 10,
        'text-allow-overlap': true,
      },
      paint: labelPaint,
    });

    map.on('click', 'clusters', function (e) {
      const f = e.features[0];
      const tap = e.originalEvent;
      map
        .getSource('rest')
        .getClusterExpansionZoom(f.properties.cluster_id)
        .then(function (z) {
          // Her tap moved the map: the list follows, as with a pan.
          map.easeTo(
            { center: f.geometry.coordinates, zoom: z, duration: 300 },
            { originalEvent: tap },
          );
        });
    });
    ['good-pins', 'rest-pins', 'sel-pin'].forEach(function (layer) {
      map.on('click', layer, function (e) {
        if (handlers.onSelect) handlers.onSelect(String(e.features[0].properties.id));
      });
    });
  }

  window.seMap = {
    // Creates (or re-attaches) the map. `onMove(lat,lng,zoom,byUser)` fires after a
    // pan, `onView(south,west,north,east,zoom)` after every move (to load the area),
    // `onSelect(id)` when a pin is tapped.
    mount: function (el, lat, lng, zoom, onMove, onView, onSelect, onError) {
      handlers = { onMove: onMove, onView: onView, onSelect: onSelect };
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
            // The base map's own shops and restaurants would duplicate SafeEat's pins
            // (with no gluten information): only stations stay.
            map.getStyle().layers.forEach(function (layer) {
              if (/^poi/.test(layer.id) && !/transit|station/.test(layer.id)) {
                map.setLayoutProperty(layer.id, 'visibility', 'none');
              }
            });
            addImages();
            addLayers();
            ready = true;
            render();
            // Only report position after the style exists, so the first search uses
            // the real camera and not a default city.
            const c = map.getCenter();
            if (handlers.onMove) handlers.onMove(c.lat, c.lng, map.getZoom(), false);
            report();
          });
          map.on('moveend', function (e) {
            if (!ready) return;
            report();
            if (!e.originalEvent) return; // the list only follows her own moves
            const c = map.getCenter();
            if (handlers.onMove) handlers.onMove(c.lat, c.lng, map.getZoom(), true);
          });
        })
        .catch(function () {
          if (onError) onError();
        });
    },
    // Adds the restaurants of a loaded area (JSON array of {id,name,lat,lng,support}).
    addArea: function (json) {
      JSON.parse(json).forEach(function (p) {
        area.set(p.id, p);
      });
      trim();
      if (!showResults) schedule();
    },
    // The list's restaurants (same shape), shown instead of the area while
    // `useResults(true)`: a typed search, or a place the catalog does not cover.
    setResults: function (json) {
      results = JSON.parse(json);
      if (showResults) schedule();
    },
    useResults: function (on) {
      if (showResults === !!on) return;
      showResults = !!on;
      schedule();
    },
    // 'all' | 'options' | 'dedicated'.
    setFilter: function (value) {
      filter = value;
      schedule();
    },
    // A place the map knows (JSON, or '' when it does not): for the preview card.
    place: function (id) {
      const p = find(id);
      return p ? JSON.stringify(p) : '';
    },
    flyTo: function (lat, lng, zoom) {
      if (map) map.jumpTo({ center: [lng, lat], zoom: zoom || map.getZoom() });
      else camera = { lat: lat, lng: lng, zoom: zoom || 14 };
    },
    // Frames the given places (JSON array): the results of a typed search.
    fitPlaces: function (json) {
      const list = JSON.parse(json);
      if (!map || !list.length) return;
      const b = new maplibregl.LngLatBounds();
      list.forEach(function (p) {
        b.extend([p.lng, p.lat]);
      });
      map.fitBounds(b, { padding: 64, maxZoom: 15, duration: 0 });
    },
    select: function (id) {
      selected = id || '';
      schedule();
    },
    center: function () {
      if (!map) return null;
      const c = map.getCenter();
      return [c.lat, c.lng];
    },
  };
})();
