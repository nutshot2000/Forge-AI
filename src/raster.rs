//! Renders a world to a PNG without a browser, so any agent that can see images can look at
//! the game. Mirrors src/web/render.js closely (tiles, sprites, animation, background, camera);
//! text (HUD, glyphs) is left out — `look` reports the HUD as text.

use crate::world::{Entity, World};
use serde_json::Value;

#[derive(Clone, Copy)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

/// CSS-ish colors: #rgb, #rgba, #rrggbb, #rrggbbaa, rgb()/rgba(), hsl(), and common names.
pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(hex) = s.strip_prefix('#') {
        let d: Vec<u8> = hex.chars().filter_map(|c| c.to_digit(16).map(|v| v as u8)).collect();
        return match d.len() {
            3 => Some(Rgba(d[0] * 17, d[1] * 17, d[2] * 17, 255)),
            4 => Some(Rgba(d[0] * 17, d[1] * 17, d[2] * 17, d[3] * 17)),
            6 => Some(Rgba(d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], 255)),
            8 => Some(Rgba(d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], d[6] * 16 + d[7])),
            _ => None,
        };
    }
    let nums = |inner: &str| -> Vec<f64> {
        inner.split(|c: char| c == ',' || c == ' ' || c == '/').filter(|t| !t.is_empty()).filter_map(|t| t.trim_end_matches('%').trim_end_matches("deg").parse().ok()).collect()
    };
    if let Some(inner) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let n = nums(inner);
        if n.len() >= 3 {
            let a = n.get(3).map_or(255.0, |a| if *a <= 1.0 { a * 255.0 } else { *a });
            return Some(Rgba(n[0] as u8, n[1] as u8, n[2] as u8, a as u8));
        }
    }
    if let Some(inner) = s.strip_prefix("hsla(").or_else(|| s.strip_prefix("hsl(")).and_then(|r| r.strip_suffix(')')) {
        let n = nums(inner);
        if n.len() >= 3 {
            let (r, g, b) = hsl(n[0], n[1] / 100.0, n[2] / 100.0);
            return Some(Rgba(r, g, b, 255));
        }
    }
    Some(match s.as_str() {
        "red" => Rgba(255, 0, 0, 255),
        "green" => Rgba(0, 128, 0, 255),
        "blue" => Rgba(0, 0, 255, 255),
        "white" => Rgba(255, 255, 255, 255),
        "black" => Rgba(0, 0, 0, 255),
        "yellow" => Rgba(255, 255, 0, 255),
        "orange" => Rgba(255, 165, 0, 255),
        "purple" => Rgba(128, 0, 128, 255),
        "pink" => Rgba(255, 192, 203, 255),
        "brown" => Rgba(139, 69, 19, 255),
        "gray" | "grey" => Rgba(128, 128, 128, 255),
        "gold" => Rgba(255, 215, 0, 255),
        "cyan" => Rgba(0, 255, 255, 255),
        "magenta" => Rgba(255, 0, 255, 255),
        "transparent" => Rgba(0, 0, 0, 0),
        _ => return None,
    })
}

fn hsl(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let f = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (f(r), f(g), f(b))
}

/// The same per-kind color the browser renderer uses when an entity has no sprite or color.
fn kind_color(kind: &str) -> Rgba {
    let mut h: u32 = 0;
    for ch in kind.chars() {
        h = h.wrapping_mul(31).wrapping_add(ch as u32);
    }
    let (r, g, b) = hsl((h % 360) as f64, 0.68, 0.62);
    Rgba(r, g, b, 255)
}

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize, c: Rgba) -> Self {
        let mut px = vec![0; w * h * 4];
        for p in px.chunks_mut(4) {
            p.copy_from_slice(&[c.0, c.1, c.2, 255]);
        }
        Canvas { w, h, px }
    }

    fn blend(&mut self, x: i64, y: i64, c: Rgba, alpha: f64) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let a = (c.3 as f64 / 255.0) * alpha;
        if a <= 0.0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        for (k, v) in [c.0, c.1, c.2].into_iter().enumerate() {
            self.px[i + k] = (self.px[i + k] as f64 * (1.0 - a) + v as f64 * a).round() as u8;
        }
    }

    fn rect(&mut self, x0: f64, y0: f64, w: f64, h: f64, c: Rgba, alpha: f64) {
        for y in (y0.floor() as i64)..((y0 + h).ceil() as i64) {
            for x in (x0.floor() as i64)..((x0 + w).ceil() as i64) {
                self.blend(x, y, c, alpha);
            }
        }
    }

    pub fn png(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity((self.w * 3 + 1) * self.h);
        for row in self.px.chunks(self.w * 4) {
            raw.push(0);
            for p in row.chunks(4) {
                raw.extend_from_slice(&p[..3]);
            }
        }
        let mut ihdr = vec![];
        ihdr.extend_from_slice(&(self.w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &zlib(&raw, self.w * 3 + 1));
        chunk(&mut out, b"IEND", &[]);
        out
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// zlib using DEFLATE with fixed Huffman codes and a few cheap match candidates: the previous
/// pixel (distance 3), the previous byte, and the pixel above (one row back). Pixel art is
/// mostly flat colors and repeated tiles, so this compresses screenshots well without a real
/// compressor.
fn zlib(data: &[u8], stride: usize) -> Vec<u8> {
    let mut bits = BitWriter::default();
    bits.put(1, 1); // final block
    bits.put(1, 2); // fixed Huffman
    let mut i = 0;
    while i < data.len() {
        let mut best = (0usize, 0usize); // (length, distance)
        for dist in [3, 1, stride] {
            if dist == 0 || dist > i || dist > 32768 {
                continue;
            }
            let mut len = 0;
            while len < 258 && i + len < data.len() && data[i + len] == data[i + len - dist] {
                len += 1;
            }
            if len > best.0 {
                best = (len, dist);
            }
        }
        let run = best.0;
        if run >= 3 {
            put_length(&mut bits, run);
            put_distance(&mut bits, best.1);
            i += run;
        } else {
            put_literal(&mut bits, data[i] as u16);
            i += 1;
        }
    }
    put_literal(&mut bits, 256); // end of block
    let mut out = vec![0x78, 0x01];
    out.extend(bits.finish());
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl BitWriter {
    /// Writes `n` bits of `v`, least significant first (DEFLATE's order for extra bits).
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= v << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Writes a Huffman code, most significant bit first.
    fn put_rev(&mut self, code: u32, len: u32) {
        let mut r = 0;
        for i in 0..len {
            r |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(r, len);
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

fn put_literal(b: &mut BitWriter, v: u16) {
    let v = v as u32;
    match v {
        0..=143 => b.put_rev(0x30 + v, 8),
        144..=255 => b.put_rev(0x190 + v - 144, 9),
        256..=279 => b.put_rev(v - 256, 7),
        _ => b.put_rev(0xC0 + v - 280, 8),
    }
}

fn put_distance(b: &mut BitWriter, dist: usize) {
    const BASE: [usize; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
    const EXTRA: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
    let i = BASE.iter().rposition(|&b| b <= dist).unwrap();
    b.put_rev(i as u32, 5);
    if EXTRA[i] > 0 {
        b.put((dist - BASE[i]) as u32, EXTRA[i]);
    }
}

fn put_length(b: &mut BitWriter, len: usize) {
    const BASE: [usize; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
    const EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
    let i = BASE.iter().rposition(|&b| b <= len).unwrap();
    put_literal(b, 257 + i as u16);
    if EXTRA[i] > 0 {
        b.put((len - BASE[i]) as u32, EXTRA[i]);
    }
}

/// A sprite frame as colors (None = transparent).
fn sprite_frame(w: &World, name: &str, frame: usize) -> Option<Vec<Vec<Option<Rgba>>>> {
    let sp = w.sprites.get(name)?;
    let rows = if frame > 0 { sp.frames.get(frame - 1).unwrap_or(&sp.pixels) } else { &sp.pixels };
    Some(
        rows.iter()
            .map(|r| r.chars().map(|ch| if ch == '.' || ch == ' ' { None } else { sp.palette.get(&ch.to_string()).and_then(|c| parse_color(c)) }).collect())
            .collect(),
    )
}

fn frame_index(w: &World, name: &str) -> usize {
    match w.sprites.get(name) {
        Some(sp) if !sp.frames.is_empty() => ((w.tick as f64 / w.tick_rate.max(0.001) * sp.fps) as usize) % (sp.frames.len() + 1),
        _ => 0,
    }
}

/// Draws a sprite into a box per `fit` (like render.js): contain (default: keep its shape,
/// centered, standing on the bottom), stretch, or tile (repeat sideways at the box's height).
fn draw_sprite_fit(c: &mut Canvas, px: &[Vec<Option<Rgba>>], x: f64, y: f64, bw: f64, bh: f64, flip: bool, alpha: f64, fit: &str) {
    let sh = px.len();
    let sw = px.iter().map(Vec::len).max().unwrap_or(0);
    if sw == 0 || sh == 0 {
        return;
    }
    let (kx, ky, tw) = match fit {
        "stretch" => (bw / sw as f64, bh / sh as f64, bw),
        "tile" => (bh / sh as f64, bh / sh as f64, bh * sw as f64 / sh as f64),
        _ => return draw_sprite(c, px, x, y, bw, bh, flip, alpha),
    };
    for dy in 0..bh.ceil() as i64 {
        for dx in 0..bw.ceil() as i64 {
            let lx = (dx as f64 + 0.5) % tw;
            let sx = ((lx / kx) as usize).min(sw - 1);
            let sy = (((dy as f64 + 0.5) / ky) as usize).min(sh - 1);
            let sx = if flip { sw - 1 - sx } else { sx };
            if let Some(Some(col)) = px.get(sy).and_then(|r| r.get(sx)) {
                c.blend((x + dx as f64).floor() as i64, (y + dy as f64).floor() as i64, *col, alpha);
            }
        }
    }
}

/// Draws a sprite into a box, keeping its shape, centered and standing on the bottom (like render.js).
fn draw_sprite(c: &mut Canvas, px: &[Vec<Option<Rgba>>], x: f64, y: f64, bw: f64, bh: f64, flip: bool, alpha: f64) {
    let sh = px.len();
    let sw = px.iter().map(Vec::len).max().unwrap_or(0);
    if sw == 0 || sh == 0 {
        return;
    }
    let k = (bw / sw as f64).min(bh / sh as f64);
    let (dw, dh) = (sw as f64 * k, sh as f64 * k);
    let (ox, oy) = (x + (bw - dw) / 2.0, y + (bh - dh));
    for dy in 0..dh.ceil() as i64 {
        for dx in 0..dw.ceil() as i64 {
            let sx = ((dx as f64 + 0.5) / k) as usize;
            let sy = ((dy as f64 + 0.5) / k) as usize;
            let sx = if flip { sw.saturating_sub(1).saturating_sub(sx) } else { sx };
            if let Some(Some(col)) = px.get(sy).and_then(|r| r.get(sx)) {
                c.blend((ox + dx as f64).floor() as i64, (oy + dy as f64).floor() as i64, *col, alpha);
            }
        }
    }
}

fn entity_sprite(e: &Entity) -> Option<String> {
    let base = e.props.get("sprite").and_then(Value::as_str).map(String::from);
    let Some(anims) = e.props.get("anims").and_then(Value::as_object) else { return base };
    let pick = |k: &str| anims.get(k).and_then(Value::as_str).map(String::from);
    let air = e.props.get("on_ground") == Some(&Value::Bool(false)) && !e.flag("on_ladder");
    let moving = e.f("vx", 0.0).abs() > 0.3;
    (if e.flag("on_ladder") { pick("climb") } else { None })
        .or(if air { if e.f("vy", 0.0) < 0.0 { pick("jump") } else { pick("fall").or(pick("jump")) } } else { None })
        .or(if moving { pick("run") } else { None })
        .or(pick("idle"))
        .or(base)
}

pub struct Shot {
    pub png: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// The map area shown, in tiles: [x, y, w, h].
    pub area: [f64; 4],
}

/// Renders the world. `camera`: what the player sees (else the whole map, or `area`).
pub fn screenshot(w: &World, camera: bool, area: Option<[f64; 4]>, scale: Option<f64>) -> Shot {
    let (mw, mh) = (w.width().max(1) as f64, w.height().max(1) as f64);
    let [ax, ay, aw, ah] = if let Some(a) = area {
        a
    } else if camera && w.camera.view[0] > 0.0 && w.camera.view[1] > 0.0 {
        let z = if w.camera.zoom > 0.0 { w.camera.zoom } else { 1.0 };
        let (vw, vh) = (w.camera.view[0] / z, w.camera.view[1] / z);
        [w.camera.x - vw / 2.0, w.camera.y - vh / 2.0, vw, vh]
    } else {
        [0.0, 0.0, mw, mh]
    };
    // Pixels per tile: the world's tile size, shrunk so big maps stay a sensible image size.
    let s = scale.unwrap_or_else(|| (w.tile_size.max(4) as f64).min(1280.0 / aw).min(960.0 / ah).max(2.0)).clamp(1.0, 64.0);
    let (cw, ch) = ((aw * s).round().max(1.0) as usize, (ah * s).round().max(1.0) as usize);
    let mut c = Canvas::new(cw, ch, Rgba(7, 8, 11, 255));
    let to_px = |x: f64, y: f64| ((x - ax) * s, (y - ay) * s);
    let bg = &w.background;
    let has_bg = !bg.sky.is_empty() || !bg.layers.is_empty();

    // Sky gradient (spanning the map's height, like the browser).
    let sky: Vec<Rgba> = bg.sky.iter().filter_map(|x| parse_color(x)).collect();
    if !sky.is_empty() {
        for py in 0..ch {
            let wy = ay + py as f64 / s;
            let t = if sky.len() == 1 { 0.0 } else { (wy / mh).clamp(0.0, 1.0) * (sky.len() - 1) as f64 };
            let (i, f) = (t.floor() as usize, t.fract());
            let (a, b) = (sky[i.min(sky.len() - 1)], sky[(i + 1).min(sky.len() - 1)]);
            let mix = |p: u8, q: u8| (p as f64 + (q as f64 - p as f64) * f).round() as u8;
            let col = Rgba(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2), 255);
            for px in 0..cw {
                c.blend(px as i64, py as i64, col, 1.0);
            }
        }
    }
    // Parallax layers.
    for l in &bg.layers {
        let Some(img) = sprite_frame(w, &l.sprite, frame_index(w, &l.sprite)) else { continue };
        let (iw, ih) = (img.iter().map(Vec::len).max().unwrap_or(1) as f64, img.len().max(1) as f64);
        let lh = l.height * s;
        let lw = lh * iw / ih;
        let y = (l.y - ay * l.parallax) * s;
        let mut x = -ax * l.parallax * s;
        if l.repeat {
            x = (x % lw + lw) % lw - lw;
            while x < cw as f64 {
                draw_sprite(&mut c, &img, x, y, lw, lh, false, 1.0);
                x += lw;
            }
        } else {
            draw_sprite(&mut c, &img, x, y, lw, lh, false, 1.0);
        }
    }

    // Tiles.
    for ty in (ay.floor().max(0.0) as i64)..((ay + ah).ceil().min(mh) as i64) {
        for tx in (ax.floor().max(0.0) as i64)..((ax + aw).ceil().min(mw) as i64) {
            let ch_ = w.tile(tx, ty);
            let (px, py) = to_px(tx as f64, ty as f64);
            let def = w.tile_def(ch_);
            let solid = w.is_solid_char(ch_);
            if let Some(img) = def.and_then(|d| d.sprite.as_ref()).and_then(|n| sprite_frame(w, n, frame_index(w, n))) {
                if !has_bg {
                    c.rect(px, py, s, s, Rgba(18, 20, 26, 255), 1.0);
                } else if ch_ != '.' {
                    if let Some(col) = w.tile_def('.').and_then(|d| d.color.as_deref()).and_then(parse_color) {
                        c.rect(px, py, s, s, col, 1.0);
                    }
                }
                draw_sprite(&mut c, &img, px, py, s, s, false, 1.0);
            } else if let Some(col) = def.and_then(|d| d.color.as_deref()).and_then(parse_color) {
                let hgt = if def.is_some_and(|d| d.platform) { (s / 4.0).max(2.0) } else { s };
                c.rect(px, py, s, hgt, col, 1.0);
            } else if solid {
                c.rect(px, py, s, s, Rgba(44, 49, 61, 255), 1.0);
                c.rect(px, py, s, (s / 8.0).max(1.0), Rgba(58, 65, 82, 255), 1.0);
            } else if def.is_some_and(|d| d.platform) {
                c.rect(px, py, s, (s / 5.0).max(2.0), Rgba(138, 122, 90, 255), 1.0);
            } else if def.is_some_and(|d| d.ladder) {
                c.rect(px + s * 0.22, py, (s / 12.0).max(1.0), s, Rgba(138, 122, 90, 255), 1.0);
                c.rect(px + s * 0.72, py, (s / 12.0).max(1.0), s, Rgba(138, 122, 90, 255), 1.0);
                for k in 1..4 {
                    c.rect(px + s * 0.22, py + s * k as f64 / 4.0, s * 0.56, (s / 12.0).max(1.0), Rgba(138, 122, 90, 255), 1.0);
                }
            } else if !has_bg && ch_ != ' ' {
                let col = if (tx + ty) % 2 == 0 { Rgba(18, 20, 26, 255) } else { Rgba(19, 21, 28, 255) };
                c.rect(px, py, s, s, col, 1.0);
            }
        }
    }

    // Entities, back to front.
    let mut ents: Vec<(&u64, &Entity)> = w.entities.iter().collect();
    ents.sort_by(|a, b| a.1.f("z", 0.0).partial_cmp(&b.1.f("z", 0.0)).unwrap().then(a.0.cmp(b.0)));
    for (_, e) in ents {
        let (bw, bh) = (e.w() * s, e.h() * s);
        let (px, py) = to_px(e.x, e.y);
        if px + bw < 0.0 || py + bh < 0.0 || px > cw as f64 || py > ch as f64 {
            continue;
        }
        let alpha = e.f("alpha", 1.0).clamp(0.0, 1.0);
        let flip = match e.props.get("flip") {
            Some(Value::Bool(b)) => *b,
            Some(Value::String(f)) if f == "auto" => e.f("vx", 0.0) < -0.05,
            _ => false,
        };
        let sprite = entity_sprite(e).and_then(|n| sprite_frame(w, &n, frame_index(w, &n)));
        match sprite {
            Some(img) => draw_sprite_fit(&mut c, &img, px, py, bw, bh, flip, alpha, e.props.get("fit").and_then(Value::as_str).unwrap_or("contain")),
            None => {
                let col = e.props.get("color").and_then(Value::as_str).and_then(parse_color).unwrap_or_else(|| kind_color(&e.kind));
                c.rect(px + bw * 0.08, py + bh * 0.08, bw * 0.84, bh * 0.84, col, alpha);
                c.rect(px + bw * 0.38, py + bh * 0.38, bw * 0.24, bh * 0.24, Rgba(13, 15, 20, 255), alpha * 0.7);
            }
        }
    }

    if w.screen.is_some() {
        c.rect(0.0, 0.0, cw as f64, ch as f64, Rgba(5, 6, 10, 255), 0.8);
    }
    Shot { png: c.png(), width: cw, height: ch, area: [ax, ay, aw, ah] }
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse() {
        assert!(matches!(parse_color("#fff"), Some(Rgba(255, 255, 255, 255))));
        assert!(matches!(parse_color("#1b2a4a"), Some(Rgba(0x1b, 0x2a, 0x4a, 255))));
        assert!(matches!(parse_color("hsl(0 100% 50%)"), Some(Rgba(255, 0, 0, 255))));
        assert!(matches!(parse_color("rgb(1, 2, 3)"), Some(Rgba(1, 2, 3, 255))));
        assert!(parse_color("nonsense").is_none());
    }

    #[test]
    fn png_is_valid_and_compresses_runs() {
        let mut c = Canvas::new(64, 32, Rgba(10, 20, 30, 255));
        c.rect(8.0, 8.0, 16.0, 8.0, Rgba(200, 50, 50, 255), 1.0);
        let png = c.png();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(png.len() < 64 * 32 * 3 / 4, "runs should compress: {} bytes", png.len());
    }
}
