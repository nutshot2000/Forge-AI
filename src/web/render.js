// forge renderer: draws a frame (map + entities + HUD) onto a canvas. Shared by the
// editor and exported games, so a game looks the same everywhere.
//
// Coordinates are in tiles; a "view" maps them to screen pixels: sx = (x - ox) * s.
(function () {
  const R = {};
  let assets = { sprites: {}, tiles: {} };
  const spriteCache = new Map();
  const facing = new Map();

  R.hue = s => { let h = 0; for (const ch of String(s)) h = (h * 31 + ch.charCodeAt(0)) >>> 0; return h % 360; };
  R.colorOf = (kind, c) => c || `hsl(${R.hue(kind)} 68% 62%)`;
  R.setAssets = (sprites, tiles) => { assets = { sprites: sprites || {}, tiles: tiles || {} }; };
  R.assets = () => assets;

  R.spriteCanvas = name => {
    const sp = assets.sprites[name];
    if (!sp) return null;
    const key = JSON.stringify(sp);
    let c = spriteCache.get(name);
    if (c && c._key === key) return c;
    const h = sp.pixels.length, w = Math.max(1, ...sp.pixels.map(r => [...r].length));
    c = document.createElement('canvas'); c.width = w; c.height = h; c._key = key;
    const g = c.getContext('2d');
    sp.pixels.forEach((row, y) => [...row].forEach((ch, x) => {
      const col = sp.palette[ch];
      if (col && ch !== '.' && ch !== ' ') { g.fillStyle = col; g.fillRect(x, y, 1, 1); }
    }));
    spriteCache.set(name, c);
    return c;
  };

  // Draws a sprite to fit inside a box (keeping its shape), centered and standing on the bottom.
  R.drawSprite = (g, name, x, y, w, h) => {
    const c = R.spriteCanvas(name);
    if (!c) return false;
    const k = Math.min(w / c.width, h / c.height);
    const dw = c.width * k, dh = c.height * k;
    g.imageSmoothingEnabled = false;
    g.drawImage(c, x + (w - dw) / 2, y + (h - dh), dw, dh);
    return true;
  };

  R.drawTile = (g, ch, px, py, size, x, y) => {
    const def = assets.tiles[ch];
    if (def && def.sprite && R.spriteCanvas(def.sprite)) {
      // See-through parts of a tile sprite show the floor tile behind them (e.g. sky).
      if (ch !== '.') R.drawTile(g, '.', px, py, size, x, y);
      else { g.fillStyle = '#12141a'; g.fillRect(px, py, size, size); }
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
  R.drawFrame = (g, frame, rows, v, opts = {}) => {
    g.fillStyle = '#07080b'; g.fillRect(0, 0, v.cw, v.ch);
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
    if (opts.hud !== false) R.drawHud(g, frame.hud, v, opts.hudTop ?? 8);
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
    if (!(e[6] && R.drawSprite(g, e[6], -w / 2, -h / 2, w, h))) {
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

  // Key names shared with the engine: lowercase, arrows as up/down/left/right, " " as space.
  R.keyName = k => ({ ' ': 'space', ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' })[k] || k.toLowerCase();
  R.isGameKey = k => ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', ' ', 'Enter', 'Shift', 'Control'].includes(k) || /^[a-z0-9]$/i.test(k);

  window.ForgeRender = R;
})();
