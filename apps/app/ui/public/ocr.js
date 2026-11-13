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

  let report = function () {};
  let worker = null;
  async function reader() {
    if (worker) return worker;
    await load();
    const base = new URL(BASE, document.baseURI).href;
    worker = Tesseract.createWorker('spa', 1 /* LSTM only: the bundled model */, {
      workerPath: base + 'worker.min.js',
      corePath: base + 'core',
      langPath: base + 'lang',
      workerBlobURL: false,
      // The model is a local file already: no copy in IndexedDB.
      cacheMethod: 'none',
      logger: function (m) {
        report(m);
      },
      errorHandler: function () {},
    }).then(
      async function (w) {
        await w.setParameters({ preserve_interword_spaces: '1' });
        return w;
      },
      function (e) {
        worker = null;
        throw e;
      },
    );
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
          return x.terminate();
        },
        function () {},
      );
    }
  };
})();
