// On-device text recognition for ingredient photos (Tesseract.js 5, Apache-2.0).
// Everything runs inside the app: the engine, the Spanish model and the photo stay
// on the phone. Nothing is sent anywhere and no paid service is used.
//
// `safeeatOcr(file, onProgress)` → Promise<{ text, confidence }>, confidence 0–100.
// `safeeatOcrStop()` frees the reader (≈100 MB of memory) when the screen closes.
/* global Tesseract -- defined by vendor/tesseract/tesseract.min.js once loaded */
'use strict';
(function () {
  const BASE = 'vendor/tesseract/';
  // The longest side a photo is reduced to: enough for label text, and a 12 MP
  // photo would take far longer and more memory for no better reading.
  const MAX_SIDE = 2000;

  let loading = null;
  function load() {
    if (window.Tesseract) return Promise.resolve();
    if (loading) return loading;
    loading = new Promise(function (resolve, reject) {
      const js = document.createElement('script');
      js.src = BASE + 'tesseract.min.js';
      js.onload = function () {
        resolve();
      };
      js.onerror = function () {
        loading = null;
        reject(new Error('ocr-load'));
      };
      document.head.appendChild(js);
    });
    return loading;
  }

  // The wasm-feature-detect SIMD probe (what Tesseract's worker uses to pick a core).
  const SIMD_PROBE = new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253,
    15, 253, 98, 11,
  ]);

  async function bytes(path) {
    // no-store: after an app update the old copy must never be reused.
    const res = await fetch(BASE + path, { cache: 'no-store' });
    if (!res.ok) throw new Error('ocr-file ' + path + ' ' + res.status);
    return res.arrayBuffer();
  }

  // Everything the worker needs is read here, on the page, and handed over as blob:
  // URLs and bytes. A worker's own requests do not go through the app's file server on
  // Android (the reader failed in the APK while working in a browser), so the worker
  // must not fetch anything itself.
  let urls = [];
  async function files() {
    let simd = false;
    try {
      simd = WebAssembly.validate(SIMD_PROBE);
    } catch {
      simd = false;
    }
    const core = simd ? 'tesseract-core-simd-lstm.wasm.js' : 'tesseract-core-lstm.wasm.js';
    const [workerJs, coreJs, spa] = await Promise.all([
      bytes('worker.min.js'),
      bytes('core/' + core),
      bytes('lang/spa.traineddata.gz'),
    ]);
    const js = function (buf) {
      const url = URL.createObjectURL(new Blob([buf], { type: 'application/javascript' }));
      urls.push(url);
      return url;
    };
    return {
      workerPath: js(workerJs),
      // Tesseract loads a core path ending in "js" as it is; a blob: URL ignores the
      // fragment.
      corePath: js(coreJs) + '#core.js',
      // Gzipped model bytes: the worker recognises and unpacks them.
      lang: { code: 'spa', data: new Uint8Array(spa) },
    };
  }

  function forget() {
    urls.forEach(function (u) {
      URL.revokeObjectURL(u);
    });
    urls = [];
  }

  let report = function () {};
  let worker = null;
  async function reader() {
    if (worker) return worker;
    worker = (async function () {
      await load();
      const f = await files();
      const w = await Tesseract.createWorker([f.lang], 1 /* LSTM only: the bundled model */, {
        workerPath: f.workerPath,
        corePath: f.corePath,
        workerBlobURL: false,
        // The model is in memory already: no copy in IndexedDB.
        cacheMethod: 'none',
        logger: function (m) {
          report(m);
        },
        errorHandler: function () {},
      });
      await w.setParameters({ preserve_interword_spaces: '1' });
      return w;
    })().catch(function (e) {
      worker = null;
      forget();
      throw e;
    });
    return worker;
  }

  // Upright (EXIF orientation) and no larger than MAX_SIDE. Falls back to the file.
  async function prepare(file) {
    if (typeof createImageBitmap !== 'function') return file;
    let bitmap;
    try {
      bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' });
    } catch {
      return file;
    }
    const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(bitmap.width * scale);
    canvas.height = Math.round(bitmap.height * scale);
    const ctx = canvas.getContext('2d');
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    if (bitmap.close) bitmap.close();
    return canvas;
  }

  window.safeeatOcr = async function (file, onProgress) {
    report = function (m) {
      if (typeof onProgress !== 'function' || !m) return;
      const reading = m.status === 'recognizing text';
      onProgress(reading ? 'read' : 'prepare', typeof m.progress === 'number' ? m.progress : 0);
    };
    try {
      const w = await reader();
      const image = await prepare(file);
      const result = await w.recognize(image);
      const data = result && result.data ? result.data : {};
      return {
        text: typeof data.text === 'string' ? data.text : '',
        confidence: typeof data.confidence === 'number' ? data.confidence : 0,
      };
    } finally {
      report = function () {};
    }
  };

  window.safeeatOcrStop = function () {
    const w = worker;
    worker = null;
    if (w) {
      w.then(
        function (x) {
          forget();
          return x.terminate();
        },
        function () {},
      );
    }
  };
})();
