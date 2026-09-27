// forge renderer: draws a frame (map + entities + HUD) onto a canvas. Shared by the
// editor and exported games, so a game looks the same everywhere.
//
// Coordinates are in tiles; a "view" maps them to screen pixels: sx = (x - ox) * s.
(function () {
  const R = {};
  let assets = { sprites: {}, tiles: {}, background: null, sounds: {} };
  let clock = { tick: 0, rate: 8 };
  const spriteCache = new Map();
  const facing = new Map();

  R.hue = s => { let h = 0; for (const ch of String(s)) h = (h * 31 + ch.charCodeAt(0)) >>> 0; return h % 360; };
  R.colorOf = (kind, c) => c || `hsl(${R.hue(kind)} 68% 62%)`;
  R.setAssets = (sprites, tiles, background, sounds) => {
    const bg = background && ((background.sky || []).length || (background.layers || []).length) ? background : null;
    assets = { sprites: sprites || {}, tiles: tiles || {}, background: bg, sounds: sounds || {} };
  };
  R.assets = () => assets;

  // Which animation frame a sprite is on right now (tied to game ticks, so it's in sync everywhere).
  R.frameIndex = name => {
    const sp = assets.sprites[name];
    if (!sp || !sp.frames || !sp.frames.length) return 0;
    return Math.floor(clock.tick / (clock.rate || 8) * (sp.fps || 8)) % (sp.frames.length + 1);
  };
  R.spriteCanvas = (name, frame = 0) => {
    const sp = assets.sprites[name];
    if (!sp) return null;
    const pixels = frame > 0 && sp.frames && sp.frames[frame - 1] ? sp.frames[frame - 1] : sp.pixels;
    const key = JSON.stringify(sp.palette) + JSON.stringify(pixels);
    const ck = name + '#' + frame;
    let c = spriteCache.get(ck);
    if (c && c._key === key) return c;
    const h = pixels.length, w = Math.max(1, ...pixels.map(r => [...r].length));
    c = document.createElement('canvas'); c.width = w; c.height = h; c._key = key;
    const g = c.getContext('2d');
    pixels.forEach((row, y) => [...row].forEach((ch, x) => {
      const col = sp.palette[ch];
      if (col && ch !== '.' && ch !== ' ') { g.fillStyle = col; g.fillRect(x, y, 1, 1); }
    }));
    spriteCache.set(ck, c);
    return c;
  };

  // Draws a sprite to fit inside a box (keeping its shape), centered and standing on the bottom.
  R.drawSprite = (g, name, x, y, w, h, frame, fit) => {
    const c = R.spriteCanvas(name, frame ?? R.frameIndex(name));
    if (!c) return false;
    g.imageSmoothingEnabled = false;
    if (fit === 'stretch') { g.drawImage(c, x, y, w, h); return true; }
    if (fit === 'tile') {
      // Repeat sideways at the box's height (platforms, bridges, long walls).
      const tw = h * c.width / c.height;
      for (let tx = 0; tx < w - 0.01; tx += tw) {
        const part = Math.min(tw, w - tx);
        g.drawImage(c, 0, 0, c.width * part / tw, c.height, x + tx, y, part, h);
      }
      return true;
    }
    const k = Math.min(w / c.width, h / c.height);
    const dw = c.width * k, dh = c.height * k;
    g.imageSmoothingEnabled = false;
    g.drawImage(c, x + (w - dw) / 2, y + (h - dh), dw, dh);
    return true;
  };

  R.drawTile = (g, ch, px, py, size, x, y) => {
    const def = assets.tiles[ch];
    // With a background, plain empty tiles are see-through: anything that isn't solid
    // and has nothing of its own to draw.
    if (assets.background) {
      const visual = def && (def.color || def.sprite || def.platform || def.ladder);
      const solid = def ? def.solid : ch === '#';
      if (!visual && !solid) return;
    }
    if (def && def.sprite && R.spriteCanvas(def.sprite)) {
      // See-through parts of a tile sprite show the floor tile (or the background) behind them.
      if (ch !== '.') R.drawTile(g, '.', px, py, size, x, y);
      else if (!assets.background) { g.fillStyle = '#12141a'; g.fillRect(px, py, size, size); }
      R.drawSprite(g, def.sprite, px, py, size, size);
      return;
    }
    if (def && def.color) {
      g.fillStyle = def.color;
      if (def.platform) {
        g.fillRect(px, py, size, Math.max(2, size / 4));
      } else g.fillRect(px, py, size, size);
      if (def.solid) { g.fillStyle = '#ffffff18'; g.fillRect(px, py, size, Math.max(2, size / 8)); }
      return;
    }
    if (ch === '#') {
      g.fillStyle = '#2c313d'; g.fillRect(px, py, size, size);
      g.fillStyle = '#3a4152'; g.fillRect(px, py, size, Math.max(2, size / 8));
      g.fillStyle = '#23272f'; g.fillRect(px, py + size - 1, size, 1);
    } else if (ch === ' ') {
      g.fillStyle = '#0a0c10'; g.fillRect(px, py, size, size);
    } else {
      g.fillStyle = (x + y) % 2 ? '#13151c' : '#12141a'; g.fillRect(px, py, size, size);
      if (def && def.platform) { g.fillStyle = '#8a7a5a'; g.fillRect(px, py, size, Math.max(2, size / 5)); }
      else if (def && def.ladder) {
        g.strokeStyle = '#8a7a5a'; g.lineWidth = Math.max(1, size / 12);
        g.beginPath(); g.moveTo(px + size * .25, py); g.lineTo(px + size * .25, py + size); g.moveTo(px + size * .75, py); g.lineTo(px + size * .75, py + size);
        for (let i = 1; i < 4; i++) { g.moveTo(px + size * .25, py + size * i / 4); g.lineTo(px + size * .75, py + size * i / 4); }
        g.stroke();
      } else if (ch !== '.') {
        g.fillStyle = '#4a5268'; g.font = `${size * .7}px monospace`; g.textAlign = 'center'; g.textBaseline = 'middle';
        g.fillText(ch, px + size / 2, py + size / 2);
      }
    }
  };

  // How tiles map to the canvas.
  //  mode 'map':    the whole map fits the canvas, then zoom/pan (pan = tile coords of the center)
  //  mode 'camera': what the player sees, from frame.camera (view [0,0] = whole map)
  R.makeView = (frame, rows, cw, ch, opts = {}) => {
    const W = Math.max(1, ...rows.map(r => [...r].length)), H = Math.max(1, rows.length);
    let cx, cy, vw, vh;
    const cam = frame.camera || {};
    if (opts.mode === 'camera' && cam.view && cam.view[0] > 0 && cam.view[1] > 0) {
      const z = cam.zoom > 0 ? cam.zoom : 1;
      vw = cam.view[0] / z; vh = cam.view[1] / z; cx = cam.x; cy = cam.y;
    } else {
      vw = W; vh = H; cx = W / 2; cy = H / 2;
      if (opts.mode !== 'camera' && opts.pan) { cx = opts.pan.x; cy = opts.pan.y; }
      if (opts.mode !== 'camera' && opts.zoom) { vw /= opts.zoom; vh /= opts.zoom; }
    }
    const margin = opts.margin ?? 0;
    let s = Math.min((cw - margin * 2) / vw, (ch - margin * 2) / vh);
    if (opts.pixelPerfect && frame.tile_size && s > frame.tile_size) s = Math.floor(s / frame.tile_size) * frame.tile_size;
    s = Math.max(1, s);
    const shake = cam.shake > 0 && opts.mode === 'camera' ? cam.shake : 0;
    const t = (opts.now || 0) / 16;
    const sx = shake ? Math.sin(t * 1.7) * shake : 0, sy = shake ? Math.cos(t * 2.3) * shake : 0;
    return { s, ox: cx + sx - cw / 2 / s, oy: cy + sy - ch / 2 / s, W, H, cw, ch };
  };
  R.toScreen = (v, x, y) => [(x - v.ox) * v.s, (y - v.oy) * v.s];
  R.toWorld = (v, px, py) => [px / v.s + v.ox, py / v.s + v.oy];

  // Draws one frame. opts: { prev (frame to interpolate from), t (0..1), now (ms), hud (bool), skip (entity id to hide) }
  // The sky gradient and parallax layers behind the map.
  R.drawBackground = (g, v) => {
    const bg = assets.background;
    g.fillStyle = '#07080b'; g.fillRect(0, 0, v.cw, v.ch);
    if (!bg) return;
    const sky = bg.sky || [];
    if (sky.length === 1) { g.fillStyle = sky[0]; g.fillRect(0, 0, v.cw, v.ch); }
    else if (sky.length > 1) {
      // The gradient spans the map's height, so it doesn't slide as the camera moves.
      const [, top] = R.toScreen(v, 0, 0), [, bottom] = R.toScreen(v, 0, v.H);
      const grad = g.createLinearGradient(0, top, 0, bottom);
      sky.forEach((c, i) => grad.addColorStop(i / (sky.length - 1), c));
      g.fillStyle = grad; g.fillRect(0, 0, v.cw, v.ch);
    }
    for (const l of bg.layers || []) {
      const c = R.spriteCanvas(l.sprite);
      if (!c) continue;
      const p = l.parallax ?? .5, S = v.s;
      const h = (l.height ?? 4) * S, w = h * c.width / c.height;
      // Parallax: the layer moves p times as fast as the map.
      const y = (l.y - v.oy * p) * S;
      let x = (-v.ox * p) * S;
      g.imageSmoothingEnabled = false;
      if (l.repeat === false) { g.drawImage(c, x, y, w, h); continue; }
      x = ((x % w) + w) % w - w;
      for (; x < v.cw; x += w) g.drawImage(c, Math.floor(x), Math.floor(y), Math.ceil(w) + 1, h);
    }
  };

  R.drawFrame = (g, frame, rows, v, opts = {}) => {
    clock = { tick: frame.tick || 0, rate: frame.tick_rate || 8 };
    R.drawBackground(g, v);
    const S = v.s;
    const x0 = Math.max(0, Math.floor(v.ox)), y0 = Math.max(0, Math.floor(v.oy));
    const x1 = Math.min(v.W - 1, Math.ceil(v.ox + v.cw / S)), y1 = Math.min(v.H - 1, Math.ceil(v.oy + v.ch / S));
    const size = Math.ceil(S);
    for (let y = y0; y <= y1; y++) {
      const row = rows[y] || '';
      for (let x = x0; x <= x1; x++) {
        const ch = opts.tileAt ? opts.tileAt(x, y, row[x]) : row[x];
        if (ch === undefined) continue;
        R.drawTile(g, ch, Math.floor((x - v.ox) * S), Math.floor((y - v.oy) * S), size, x, y);
      }
    }
    const t = opts.t ?? 1;
    const ease = t;
    const before = new Map((opts.prev ? opts.prev.ents : []).map(e => [e[0], e]));
    const list = [...frame.ents].sort((a, b) => ((a[7] || {}).z || 0) - ((b[7] || {}).z || 0) || a[0] - b[0]);
    for (const e of list) {
      if (opts.skip === e[0]) continue;
      const x = e[2], y = e[3], ex = e[7] || {};
      const p = before.get(e[0]);
      let dx = x, dy = y;
      if (p && Math.abs(p[2] - x) < 4 && Math.abs(p[3] - y) < 4) { dx = p[2] + (x - p[2]) * ease; dy = p[3] + (y - p[3]) * ease; }
      R.drawEntity(g, e, dx, dy, v);
    }
    R.drawLabels(g, list, v);
    R.drawFx(g, v, opts.now ?? performance.now());
    if (opts.hud !== false) R.drawHud(g, frame.hud, v, opts.hudTop ?? 8);
    if (frame.effect) R.drawEffect(g, frame.effect, v, frame.tick || 0);
    if (frame.screen && opts.screen !== false) R.drawScreen(g, frame.screen, v, frame.tick || 0);
    // UI goes on top, so title and game-over screens can have buttons.
    R.uiRects = R.drawUi(g, frame.ui, v, opts.mouse);
  };

  // ---- labels above entities ----
  R.drawLabels = (g, ents, v) => {
    for (const e of ents) {
      const ex = e[7] || {};
      if (ex.label === undefined || ex.label === null || ex.label === '') continue;
      const [x, y] = R.toScreen(v, e[2] + (ex.w ?? 1) / 2, e[3]);
      const fs = Math.max(10, Math.min(18, v.s * 0.38));
      g.font = `bold ${fs}px system-ui, sans-serif`;
      const text = String(ex.label), tw = g.measureText(text).width + 8;
      g.fillStyle = '#000a'; g.beginPath(); g.roundRect(x - tw / 2, y - fs - 8, tw, fs + 4, 4); g.fill();
      g.fillStyle = ex.label_color || '#fff'; g.textAlign = 'center'; g.textBaseline = 'middle';
      g.fillText(text, x, y - fs / 2 - 6);
    }
  };

  // ---- particles and floating text (cosmetic; they don't affect the game) ----
  const PRESETS = {
    explosion: { count: 34, speed: 6, colors: ['#fff2b0', '#ffd060', '#ff8a3d', '#ff4d3d'], life: 650, size: 0.22, gravity: 4, drag: 2.5 },
    dust: { count: 10, speed: 1.8, colors: ['#c9b89c', '#8f8170'], life: 450, size: 0.16, gravity: 3, drag: 3, up: 0.8 },
    sparkle: { count: 14, speed: 1.5, colors: ['#fff6b0', '#ffffff', '#ffe066'], life: 750, size: 0.12, gravity: -0.6, drag: 1.5, twinkle: true },
    smoke: { count: 14, speed: 0.8, colors: ['#6b6f7a', '#8a8f99', '#4b4f58'], life: 1100, size: 0.3, gravity: -1.5, drag: 1, grow: 1.5 },
    hit: { count: 10, speed: 5, colors: ['#ffffff', '#ff6b6b'], life: 260, size: 0.12, gravity: 0, drag: 5 },
    confetti: { count: 40, speed: 6, colors: ['#ff5f6d', '#ffd060', '#5fd39a', '#7aa2ff', '#b48cff'], life: 1500, size: 0.15, gravity: 8, drag: 1.2, up: 1 },
    splash: { count: 16, speed: 4, colors: ['#8ec5ff', '#2f6db3', '#ffffff'], life: 600, size: 0.14, gravity: 14, drag: 0.5, up: 1.2 },
    trail: { count: 1, speed: 0.2, colors: ['#ffffffaa'], life: 300, size: 0.1, gravity: 0, drag: 0 },
    fire: { count: 12, speed: 1.2, colors: ['#fff2b0', '#ffd060', '#ff8a3d', '#ff4d3d'], life: 520, size: 0.2, gravity: -3, drag: 1, shrink: true },
  };
  let parts = [], texts = [], lastFx = performance.now();
  R.spawnFx = (d, now = performance.now()) => {
    if (!d) return;
    if (d.fx === 'text') { texts.push({ x: d.x, y: d.y, text: String(d.text), color: d.color || '#ffffff', t0: now }); return; }
    const p = { ...(PRESETS[d.preset] || PRESETS.hit), ...(d.opts || {}) };
    if (d.opts && d.opts.color) p.colors = [d.opts.color];
    const n = Math.min(300, p.count | 0);
    for (let i = 0; i < n; i++) {
      const a = Math.random() * Math.PI * 2, sp = p.speed * (0.35 + Math.random() * 0.65);
      parts.push({ x: d.x, y: d.y, vx: Math.cos(a) * sp, vy: Math.sin(a) * sp - (p.up || 0) * p.speed * Math.random(),
        life: p.life * (0.6 + Math.random() * 0.4), age: 0, size: p.size * (0.6 + Math.random() * 0.8),
        color: p.colors[(Math.random() * p.colors.length) | 0], g: p.gravity, drag: p.drag, grow: p.grow || 0, shrink: p.shrink, twinkle: p.twinkle });
    }
    if (parts.length > 3000) parts.splice(0, parts.length - 3000);
  };
  R.drawFx = (g, v, now) => {
    const dt = Math.min(0.05, Math.max(0, (now - lastFx) / 1000));
    lastFx = now;
    const S = v.s;
    parts = parts.filter(p => (p.age += dt * 1000) < p.life);
    for (const p of parts) {
      p.vy += p.g * dt;
      const k = Math.max(0, 1 - p.drag * dt);
      p.vx *= k; p.vy *= k;
      p.x += p.vx * dt; p.y += p.vy * dt;
      const t = p.age / p.life;
      const size = p.size * (1 + p.grow * t) * (p.shrink ? 1 - t : 1);
      g.globalAlpha = Math.max(0, 1 - t) * (p.twinkle ? 0.5 + 0.5 * Math.sin(p.age / 40) : 1);
      g.fillStyle = p.color;
      const [sx, sy] = R.toScreen(v, p.x, p.y), px = Math.max(1, size * S);
      g.fillRect(Math.round(sx - px / 2), Math.round(sy - px / 2), Math.ceil(px), Math.ceil(px));
    }
    g.globalAlpha = 1;
    texts = texts.filter(t => now - t.t0 < 900);
    for (const t of texts) {
      const k = (now - t.t0) / 900;
      const [sx, sy] = R.toScreen(v, t.x, t.y - k * 1.2);
      const fs = Math.max(12, Math.min(26, S * 0.55));
      g.font = `bold ${fs}px ui-monospace, Consolas, monospace`; g.textAlign = 'center'; g.textBaseline = 'middle';
      g.globalAlpha = 1 - k * k;
      g.fillStyle = '#000'; g.fillText(t.text, sx + 2, sy + 2);
      g.fillStyle = t.color; g.fillText(t.text, sx, sy);
    }
    g.globalAlpha = 1;
  };

  // ---- UI elements from scripts ----
  const ANCHOR = { topleft: [0, 0], top: [0.5, 0], topright: [1, 0], left: [0, 0.5], center: [0.5, 0.5], right: [1, 0.5], bottomleft: [0, 1], bottom: [0.5, 1], bottomright: [1, 1] };
  const uiFont = (e, fs) => `${e.bold === false ? '' : 'bold '}${fs}px ${e.font === 'mono' ? 'ui-monospace, Consolas, monospace' : 'system-ui, sans-serif'}`;
  R.drawUi = (g, ui, v, mouse) => {
    const rects = [];
    if (!ui) return rects;
    const els = Object.entries(ui).filter(([, e]) => e && e.visible !== false).sort((a, b) => (a[1].z || 0) - (b[1].z || 0));
    for (const [name, e] of els) {
      const world = e.space === 'world';
      const unitW = world ? v.s : v.cw / 100, unitH = world ? v.s : v.ch / 100;
      let [x, y] = world ? R.toScreen(v, e.x ?? 0, e.y ?? 0) : [(e.x ?? 0) * unitW, (e.y ?? 0) * unitH];
      const type = e.type || 'text';
      const fs = Math.max(9, (e.size ?? (world ? 0.5 : 4)) * unitH);
      let w = (e.w ?? 0) * unitW, h = (e.h ?? 0) * unitH;
      if (type === 'text' && !e.w) { g.font = uiFont(e, fs); w = Math.max(...String(e.text ?? '').split('\n').map(l => g.measureText(l).width)); h = fs * 1.25 * String(e.text ?? '').split('\n').length; }
      if (type === 'button' && !e.w) { g.font = uiFont(e, fs); w = g.measureText(String(e.text ?? '')).width + fs * 1.6; h = fs * 2; }
      const [ax, ay] = ANCHOR[e.anchor || 'topleft'] || [0, 0];
      x -= w * ax; y -= h * ay;
      g.globalAlpha = e.alpha ?? 1;
      const hover = mouse && mouse[0] >= x && mouse[0] <= x + w && mouse[1] >= y && mouse[1] <= y + h;
      if (type === 'panel' || type === 'button') {
        g.fillStyle = e.color || (type === 'button' ? (hover ? '#3a4a7a' : '#2a3558') : '#0d1018d9');
        g.beginPath(); g.roundRect(x, y, w, h, e.radius ?? Math.min(12, h / 4)); g.fill();
        if (e.border || type === 'button') { g.strokeStyle = e.border || (hover ? '#9fb8ff' : '#6d83c4'); g.lineWidth = 2; g.stroke(); }
        if (type === 'button') rects.push({ name, x, y, w, h });
      }
      if (type === 'bar') {
        const frac = Math.max(0, Math.min(1, (e.value ?? 0) / (e.max || 1)));
        g.fillStyle = e.bg || '#000b'; g.fillRect(x, y, w, h);
        g.fillStyle = e.color || (frac > 0.5 ? '#5fd39a' : frac > 0.25 ? '#f0b35a' : '#ff6b6b'); g.fillRect(x, y, w * frac, h);
        g.strokeStyle = '#000d'; g.lineWidth = 1; g.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
      }
      if (type === 'image' && e.sprite) R.drawSprite(g, e.sprite, x, y, w || fs * 2, h || fs * 2, undefined, e.fit);
      const text = type === 'text' || type === 'button' ? e.text : (e.text || null);
      if (text !== undefined && text !== null && text !== '') {
        g.font = uiFont(e, fs);
        g.textBaseline = 'middle';
        const align = type === 'button' ? 'center' : (e.align || 'left');
        g.textAlign = align;
        const tx = align === 'center' ? x + w / 2 : align === 'right' ? x + w : x;
        const lines = String(text).split('\n');
        lines.forEach((line, i) => {
          const ty = type === 'text' ? y + fs / 2 + i * fs * 1.25 : y + h / 2 + (i - (lines.length - 1) / 2) * fs * 1.25;
          if (e.shadow !== false) { g.fillStyle = '#000c'; g.fillText(line, tx + Math.max(1, fs / 12), ty + Math.max(1, fs / 12)); }
          g.fillStyle = type === 'text' ? (e.color || '#fff') : (e.text_color || '#fff');
          g.fillText(line, tx, ty);
        });
      }
      g.globalAlpha = 1;
    }
    return rects;
  };
  R.uiRects = [];
  R.uiHit = (px, py) => (R.uiRects || []).find(r => px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h);

  // ---- fades and flashes ----
  R.drawEffect = (g, fx, v, tick) => {
    const t = Math.max(0, (tick - fx.start) / Math.max(1, fx.dur));
    const a = fx.kind === 'fade_out' ? Math.min(1, t) : fx.kind === 'fade_in' ? Math.max(0, 1 - t) : Math.max(0, 1 - t) * 0.8;
    if (a <= 0) return;
    g.globalAlpha = a; g.fillStyle = fx.color || '#000'; g.fillRect(0, 0, v.cw, v.ch); g.globalAlpha = 1;
  };

  // Title / pause / game over overlays set by show_screen().
  R.drawScreen = (g, sc, v, tick) => {
    const W = v.cw, H = v.ch;
    g.fillStyle = '#05060acc'; g.fillRect(0, 0, W, H);
    g.textAlign = 'center'; g.textBaseline = 'middle';
    const big = Math.round(Math.max(22, Math.min(64, W / 14)));
    g.font = `bold ${big}px ui-monospace, Consolas, monospace`;
    g.fillStyle = '#000'; g.fillText(sc.title, W / 2 + 3, H * 0.4 + 3);
    g.fillStyle = '#fff'; g.fillText(sc.title, W / 2, H * 0.4);
    if (sc.text) {
      const fs = Math.round(Math.max(13, big * 0.36));
      g.font = `${fs}px system-ui, sans-serif`; g.fillStyle = '#d9dde8';
      String(sc.text).split('\n').forEach((line, i) => g.fillText(line, W / 2, H * 0.4 + big * 0.9 + i * fs * 1.4));
    }
    if (sc.prompt && Math.floor(tick / 20) % 2 === 0) {
      const fs = Math.round(Math.max(13, big * 0.34));
      g.font = `bold ${fs}px ui-monospace, Consolas, monospace`; g.fillStyle = '#f5c542';
      g.fillText(sc.prompt, W / 2, H * 0.72);
    }
  };

  R.drawEntity = (g, e, x, y, v, alpha = 1) => {
    const ex = e[7] || {}, S = v.s;
    const w = (ex.w ?? 1) * S, h = (ex.h ?? 1) * S;
    const px = (x - v.ox) * S, py = (y - v.oy) * S;
    if (px + w < 0 || py + h < 0 || px > v.cw || py > v.ch) return;
    let flip = ex.flip === true;
    if (ex.flip === 'auto') {
      const vx = ex.vx || 0;
      if (Math.abs(vx) > 0.05) facing.set(e[0], vx < 0);
      flip = facing.get(e[0]) || false;
    }
    g.save();
    g.globalAlpha = alpha * (ex.alpha ?? 1);
    g.translate(px + w / 2, py + h / 2);
    if (ex.angle) g.rotate(ex.angle * Math.PI / 180);
    const sc = ex.scale ?? 1;
    g.scale(flip ? -sc : sc, sc);
    // anims: pick the sprite from how the entity is moving.
    let sprite = e[6];
    const a = ex.anims;
    if (a && typeof a === 'object') {
      const air = ex.on_ground === false && !ex.on_ladder;
      const moving = Math.abs(ex.vx || 0) > 0.3;
      sprite = (ex.on_ladder && a.climb) || (air && ((ex.vy || 0) < 0 ? a.jump : (a.fall || a.jump))) || (moving && a.run) || a.idle || sprite;
    }
    if (!(sprite && R.drawSprite(g, sprite, -w / 2, -h / 2, w, h, undefined, ex.fit))) {
      // No sprite: a colored rounded box with the glyph.
      const bw = w * .84, bh = h * .84;
      g.fillStyle = R.colorOf(e[4], e[5]);
      g.beginPath(); g.roundRect(-bw / 2, -bh / 2, bw, bh, Math.min(bw, bh) * .2); g.fill();
      g.fillStyle = '#0d0f14'; g.font = `bold ${Math.min(w, h) * .55}px monospace`; g.textAlign = 'center'; g.textBaseline = 'middle';
      if (flip) g.scale(-1, 1);
      g.fillText(e[1], 0, 1);
    }
    g.restore();
  };

  R.drawHud = (g, hud, v, top = 8) => {
    const lines = Object.values(hud || {});
    if (!lines.length) return;
    const fs = Math.round(Math.max(12, Math.min(20, v.ch / 32)));
    g.font = `bold ${fs}px ui-monospace, Consolas, monospace`;
    const lh = fs * 1.35;
    const w = Math.max(...lines.map(s => g.measureText(s).width)) + 18;
    g.fillStyle = '#000a'; g.beginPath(); g.roundRect(8, top, w, lines.length * lh + 10, 7); g.fill();
    g.fillStyle = '#fff'; g.textAlign = 'left'; g.textBaseline = 'top';
    lines.forEach((s, i) => g.fillText(s, 17, top + 5 + i * lh));
  };

  // ---- sound: recipes synthesized with Web Audio ----
  let audio = null, muted = false;
  R.setMuted = m => { muted = m; };
  R.unlockAudio = () => {
    try { if (!audio) audio = new (window.AudioContext || window.webkitAudioContext)(); if (audio.state === 'suspended') audio.resume(); } catch (e) {}
  };
  R.playSound = s => {
    if (muted || !s) return;
    R.unlockAudio();
    if (!audio || audio.state !== 'running') return;
    const t = audio.currentTime, dur = Math.max(0.01, s.dur ?? 0.15), vol = Math.min(1, Math.max(0, s.vol ?? 0.3));
    const gain = audio.createGain();
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.linearRampToValueAtTime(vol, t + Math.max(0.001, s.attack ?? 0.005));
    gain.gain.exponentialRampToValueAtTime(0.0001, t + dur);
    gain.connect(audio.destination);
    let src;
    if (s.wave === 'noise') {
      const n = Math.floor(audio.sampleRate * dur), buf = audio.createBuffer(1, n, audio.sampleRate), d = buf.getChannelData(0);
      // Pitch shapes the noise: hold each random value for a while at low "frequencies".
      const hold = Math.max(1, Math.floor(audio.sampleRate / Math.max(20, (s.freq ?? 440) * 4)));
      let v = 0;
      for (let i = 0; i < n; i++) { if (i % hold === 0) v = Math.random() * 2 - 1; d[i] = v; }
      src = audio.createBufferSource(); src.buffer = buf;
    } else {
      src = audio.createOscillator();
      src.type = s.wave === 'saw' ? 'sawtooth' : (s.wave || 'square');
      const f0 = Math.max(20, s.freq ?? 440);
      src.frequency.setValueAtTime(f0, t);
      if (s.slide) src.frequency.linearRampToValueAtTime(Math.max(20, f0 + s.slide * dur), t + dur);
      (s.arp || []).forEach((semi, i) => src.frequency.setValueAtTime(f0 * Math.pow(2, semi / 12), t + i * (s.arp_speed ?? 0.06)));
      if (s.vibrato && s.vibrato_rate) {
        const lfo = audio.createOscillator(), depth = audio.createGain();
        lfo.frequency.value = s.vibrato_rate; depth.gain.value = s.vibrato;
        lfo.connect(depth); depth.connect(src.frequency); lfo.start(t); lfo.stop(t + dur);
      }
    }
    src.connect(gain); src.start(t); src.stop(t + dur + 0.02);
  };
  R.playSfx = name => R.playSound(assets.sounds[name]);

  // Key names shared with the engine: lowercase, arrows as up/down/left/right, " " as space.
  R.keyName = k => ({ ' ': 'space', ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' })[k] || k.toLowerCase();
  R.isGameKey = k => ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', ' ', 'Enter', 'Shift', 'Control'].includes(k) || /^[a-z0-9]$/i.test(k);

  window.ForgeRender = R;
})();
