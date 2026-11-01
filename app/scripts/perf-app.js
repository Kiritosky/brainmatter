// Runs inside the real app's window, put there by perf-app.sh. Unlocks the invented
// vault and reports how it goes, as one JSON line per measurement.
(async () => {
  if (window.__bmPerf) return;
  window.__bmPerf = true;

  const PASSWORD = 'perf-messung-1234';
  // The load of a Retina display on one that is not: four times the pixels to fill.
  if (window.__bmPerfDpr) Object.defineProperty(window, 'devicePixelRatio', { value: window.__bmPerfDpr });
  const invoke = window.__TAURI_INTERNALS__.invoke;
  // What the app itself asks for, phase by phase: idling, it should ask for nothing.
  let calls = {};
  const nativeFetch = window.fetch;
  window.fetch = async (url, ...rest) => {
    const command = String(url).split('/').pop();
    const started = performance.now();
    const response = await nativeFetch(url, ...rest);
    const entry = (calls[command] ??= { count: 0, ms: 0 });
    entry.count++;
    entry.ms += Math.round(performance.now() - started);
    return response;
  };

  // Someone at this Mac must not change what is being measured.
  let ignored = 0;
  for (const type of ['pointerdown', 'pointerup', 'pointermove', 'mousedown', 'mouseup', 'mousemove', 'click', 'dblclick', 'wheel', 'keydown', 'keyup', 'contextmenu']) {
    window.addEventListener(
      type,
      (e) => {
        if (!e.isTrusted) return;
        ignored++;
        e.stopImmediatePropagation();
        e.preventDefault();
      },
      { capture: true, passive: false },
    );
  }

  const report = (name, values = {}) => {
    const line = JSON.stringify({ name, at: Date.now(), ...values, calls, ignored });
    calls = {};
    ignored = 0;
    return invoke('plugin:event|emit', { event: 'bm-perf', payload: line });
  };
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const frame = () => new Promise((resolve) => window.requestAnimationFrame(resolve));

  async function until(check, timeoutMs, what) {
    const end = performance.now() + timeoutMs;
    for (;;) {
      const value = check();
      if (value) return value;
      if (performance.now() > end) throw new Error(`timed out waiting for ${what}`);
      await sleep(16);
    }
  }

  // Counts what actually reaches the GPU: a steady frame rate means nothing if nothing is drawn.
  let draws = 0;
  let instances = 0;
  const proto = WebGL2RenderingContext.prototype;
  // The first draw compiles the shaders, and waits for them here.
  let shaderMs = 0;
  const programParameter = proto.getProgramParameter;
  proto.getProgramParameter = function (...args) {
    const started = performance.now();
    const result = programParameter.apply(this, args);
    shaderMs += performance.now() - started;
    return result;
  };
  for (const method of ['drawArrays', 'drawElements', 'drawArraysInstanced', 'drawElementsInstanced']) {
    const original = proto[method];
    proto[method] = function (...args) {
      draws++;
      if (method.endsWith('Instanced')) instances += args.at(-1);
      return original.apply(this, args);
    };
  }

  // Script time in each frame, to tell work done here from a GPU that cannot keep up.
  let scriptMs = 0;
  let glMs = 0;
  const nativeFrame = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback) =>
    nativeFrame((time) => {
      const started = performance.now();
      callback(time);
      scriptMs += performance.now() - started;
    });
  for (const method of ['drawArrays', 'drawElements', 'drawArraysInstanced', 'drawElementsInstanced']) {
    const counted = proto[method];
    proto[method] = function (...args) {
      const started = performance.now();
      const result = counted.apply(this, args);
      glMs += performance.now() - started;
      return result;
    };
  }

  const quantile = (sorted, q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
  const round = (x) => Math.round(x * 10) / 10;

  /** Frame times while `each` keeps the graph busy. */
  async function frames(ms, each) {
    const times = [];
    let last = performance.now();
    const end = last + ms;
    const startDraws = draws;
    const startScript = scriptMs;
    const startGl = glMs;
    let peak = 0;
    let previous = instances;
    while (performance.now() < end) {
      each?.(times.length);
      await frame();
      const now = performance.now();
      times.push(now - last);
      last = now;
      peak = Math.max(peak, instances - previous);
      previous = instances;
    }
    const sorted = [...times].sort((a, b) => a - b);
    const total = times.reduce((a, b) => a + b, 0);
    return {
      fps: round(times.length / (total / 1000)),
      p50: round(quantile(sorted, 0.5)),
      p95: round(quantile(sorted, 0.95)),
      max: round(sorted.at(-1)),
      over34ms: times.filter((t) => t > 34).length,
      frames: times.length,
      drawsPerFrame: round((draws - startDraws) / times.length),
      peakInstances: peak,
      scriptMsPerFrame: round((scriptMs - startScript) / times.length),
      drawCallMsPerFrame: round((glMs - startGl) / times.length),
    };
  }

  try {
    const field = await until(() => document.querySelector('input[type="password"]'), 30000, 'the lock screen');
    await report('lock_screen', {
      timeOrigin: performance.timeOrigin,
      dpr: devicePixelRatio,
      width: innerWidth,
      height: innerHeight,
      agent: navigator.userAgent,
    });
    localStorage.setItem('bm.tourSeen', '1');

    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, PASSWORD);
    field.dispatchEvent(new Event('input', { bubbles: true }));
    const unlocked = performance.now();
    field.form.requestSubmit();

    const canvas = await until(() => document.querySelector('[data-tour="graph"] canvas:last-of-type'), 60000, 'the graph');
    const sinceUnlock = () => Math.round(performance.now() - unlocked);
    await report('workspace', { ms: sinceUnlock() });
    await until(() => draws > 0, 120000, 'a first draw');
    await report('first_draw', { ms: sinceUnlock(), shaderMs: Math.round(shaderMs) });

    // Whatever else came up, such as what is new in this version.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));

    // Everything on screen and quiet: nothing took longer than a few frames for two seconds.
    let peak = 0;
    let peakAt = 0;
    let calm = 0;
    let longest = 0;
    let previous = instances;
    let last = performance.now();
    const end = last + 180000;
    while (calm < 2000 && last < end) {
      await frame();
      const now = performance.now();
      longest = Math.max(longest, now - last);
      calm = now - last > 50 ? 0 : calm + (now - last);
      last = now;
      if (instances - previous > peak * 1.02) {
        peak = instances - previous;
        peakAt = sinceUnlock();
      }
      previous = instances;
    }
    await report('settled', { ms: sinceUnlock(), allDrawnAt: peakAt, instances: peak, longestFrameMs: Math.round(longest) });

    const started = performance.now();
    const snapshot = await invoke('graph', { project: null });
    const received = performance.now();
    await report('graph_ipc', {
      ms: Math.round(received - started),
      nodes: snapshot.nodes.length,
      edges: snapshot.edges.length,
      megabytes: round(JSON.stringify(snapshot).length / 1e6),
    });

    await report('idle', await frames(3000));
    // Long after everything has moved into place: nothing here should still be drawing.
    await sleep(20000);
    await report('idle_later', await frames(3000));

    const box = canvas.getBoundingClientRect();
    const at = (x, y) => ({ bubbles: true, cancelable: true, clientX: box.x + box.width * x, clientY: box.y + box.height * y });
    const sweep = (i) => (Math.sin(i / 25) + 1) / 2;

    // Zooming in and out over the whole graph redraws every entry and link, every frame.
    const zoom = (i) => {
      if (i % 3 === 0) canvas.dispatchEvent(new WheelEvent('wheel', { ...at(0.5, 0.5), deltaY: i % 240 < 120 ? -40 : 40 }));
    };
    await report('zoom', await frames(8000, zoom));

    // Dragging the view.
    // From a corner, where nothing is in the way that would be picked up instead.
    canvas.dispatchEvent(new MouseEvent('mousedown', { ...at(0.01, 0.01), button: 0 }));
    const pan = (i) => canvas.dispatchEvent(new MouseEvent('mousemove', at(0.01 + sweep(i) * 0.1, 0.01)));
    await report('pan', await frames(5000, pan));
    canvas.dispatchEvent(new MouseEvent('mouseup', { ...at(0.01, 0.01), button: 0 }));
    document.dispatchEvent(new MouseEvent('mouseup', { ...at(0.01, 0.01), button: 0 }));
    await sleep(500);

    // The pointer across the entries, which lights up neighbours and may push them aside.
    const hover = (i) => canvas.dispatchEvent(new MouseEvent('mousemove', at(0.1 + sweep(i) * 0.8, 0.3 + ((i % 200) / 200) * 0.4)));
    await report('hover', await frames(6000, hover));

    let status = await invoke('model_status');
    if (status.state === 'ready') {
      const indexing = performance.now();
      let reported = 0;
      const during = [];
      while (status.indexed < status.total && performance.now() - indexing < 1500000) {
        // What using the app feels like meanwhile.
        if (reported++ % 6 === 0) during.push((await frames(2000, zoom)).p95);
        await sleep(5000);
        status = await invoke('model_status');
        await report('indexing', { indexed: status.indexed, total: status.total, ms: Math.round(performance.now() - indexing) });
      }
      await report('indexed', { indexed: status.indexed, total: status.total, ms: Math.round(performance.now() - indexing), zoomP95WhileIndexing: during });

      const search = (text) =>
        invoke('search', { query: { text, project: null, include_subprojects: true, kinds: [], tags: [], limit: 20 } });
      // Loads the model, which only starts on the first search that asks for it.
      await search('datenbank verbindung');
      await sleep(4000);
      const first = ['daten', 'server', 'projekt', 'memory', 'layout', 'notiz', 'graphen', 'verbindung', 'index', 'testplan'];
      const second = ['code', 'idee', 'bank', 'such', 'plan', 'mory', 'tiz', 'phen', 'out', 'jekt'];
      const times = [];
      for (const a of first) {
        for (const b of second) {
          const started = performance.now();
          await search(`${a} ${b}`);
          times.push(performance.now() - started);
        }
      }
      times.sort((a, b) => a - b);
      await report('search_hybrid', { searches: times.length, p50: round(quantile(times, 0.5)), p95: round(quantile(times, 0.95)), max: round(times.at(-1)) });
    }
    await report('done');
  } catch (e) {
    await report('failed', { error: String(e) });
  }
})();
