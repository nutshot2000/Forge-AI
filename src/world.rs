//! Pure world data. Everything here is plain, cloneable and serializable, so
//! snapshots are just `clone()` and a saved world is a readable text folder.
//!
//! Units: one tile = 1.0. Entity positions (`x`, `y`) are the top-left corner of the
//! entity's box (`w` × `h`, default 1 × 1). Grid games use whole numbers and never see
//! a fraction; physics games move in fractions of a tile.

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

const EVENT_CAP: usize = 20_000;
const DIRS: [(i64, i64); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Whole numbers as JSON integers (so grid worlds read `"x": 3`), others as floats.
pub fn num(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

/// Like `num`, rounded to 1/1000 of a tile for output people and agents read.
pub fn num_out(v: f64) -> Value {
    num((v * 1000.0).round() / 1000.0)
}

fn ser_num<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    num(*v).serialize(s)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub kind: String,
    #[serde(serialize_with = "ser_num")]
    pub x: f64,
    #[serde(serialize_with = "ser_num")]
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// Free-form properties (hp, sprite, vx, w, h...). Flattened into the entity in JSON.
    #[serde(flatten)]
    pub props: BTreeMap<String, Value>,
}

impl Entity {
    /// The `glyph` prop, else the first letter of the kind.
    pub fn glyph(&self) -> char {
        self.props
            .get("glyph")
            .and_then(Value::as_str)
            .and_then(|s| s.chars().next())
            .or_else(|| self.kind.chars().next())
            .unwrap_or('?')
    }

    pub fn f(&self, key: &str, default: f64) -> f64 {
        self.props.get(key).and_then(Value::as_f64).unwrap_or(default)
    }

    pub fn flag(&self, key: &str) -> bool {
        match self.props.get(key) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Null) | None => false,
            Some(Value::Number(n)) => n.as_f64() != Some(0.0),
            Some(Value::String(s)) => !s.is_empty(),
            Some(_) => true,
        }
    }

    pub fn w(&self) -> f64 {
        self.f("w", 1.0)
    }

    pub fn h(&self) -> f64 {
        self.f("h", 1.0)
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w() / 2.0, self.y + self.h() / 2.0)
    }

    /// The tile the entity's center is on.
    pub fn cell(&self) -> (i64, i64) {
        let (cx, cy) = self.center();
        (cx.floor() as i64, cy.floor() as i64)
    }

    /// Axis-aligned overlap (touching edges don't count).
    pub fn overlaps(&self, o: &Entity) -> bool {
        const E: f64 = 1e-6;
        self.x < o.x + o.w() - E && o.x < self.x + self.w() - E && self.y < o.y + o.h() - E && o.y < self.y + self.h() - E
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub seq: u64,
    pub tick: u64,
    pub kind: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

/// What a map character means. Characters without a definition are floor, except `#` (wall).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TileDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub solid: bool,
    /// One-way platform: solid only when landing on it from above.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub platform: bool,
    /// Climbable; bodies overlapping it get `on_ladder`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ladder: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprite: Option<String>,
}

/// Pixel art as text: each row is a string, each char a palette key. `.` and ` ` are transparent.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Sprite {
    pub palette: BTreeMap<String, String>,
    pub pixels: Vec<String>,
}

/// Keyboard state for scripts. `down` = held now, `pressed` = went down since the last tick.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub down: BTreeSet<String>,
    pub pressed: BTreeSet<String>,
    /// Keys tapped by an agent: released automatically after one tick.
    pub taps: BTreeSet<String>,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// World-wide physics settings (units: tiles and seconds).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Physics {
    /// Downward acceleration in tiles/s² (0 for top-down games).
    pub gravity: f64,
    /// Terminal falling speed in tiles/s.
    pub max_fall: f64,
}

impl Default for Physics {
    fn default() -> Self {
        Physics { gravity: 0.0, max_fall: 30.0 }
    }
}

/// What the player sees. `view` is the visible area in tiles ([0, 0] = the whole map).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Camera {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow: Option<u64>,
    /// Center of the view, in tiles.
    pub x: f64,
    pub y: f64,
    pub view: [f64; 2],
    pub zoom: f64,
    /// 0..1: how quickly the camera catches up with its target each tick (1 = instantly).
    pub lerp: f64,
    /// Keep the view inside the map.
    pub bounds: bool,
    /// Screen shake strength in tiles; decays over time.
    pub shake: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { follow: None, x: 0.0, y: 0.0, view: [0.0, 0.0], zoom: 1.0, lerp: 0.2, bounds: true, shake: 0.0 }
    }
}

fn default_tick_rate() -> f64 {
    8.0
}
fn is_default_tick_rate(v: &f64) -> bool {
    *v == 8.0
}
fn default_tile_size() -> u32 {
    16
}
fn is_default_tile_size(v: &u32) -> bool {
    *v == 16
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct World {
    #[serde(default)]
    pub tick: u64,
    /// Simulation steps per second. Physics uses dt = 1 / tick_rate, so results don't
    /// depend on how fast the editor plays it back.
    #[serde(default = "default_tick_rate", skip_serializing_if = "is_default_tick_rate")]
    pub tick_rate: f64,
    /// Pixels per tile when the game is shown at 1× (sprites are drawn to fit their box).
    #[serde(default = "default_tile_size", skip_serializing_if = "is_default_tile_size")]
    pub tile_size: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub physics: Physics,
    #[serde(default, skip_serializing_if = "is_default")]
    pub camera: Camera,
    /// RNG state. Part of the world, so runs are deterministic and snapshots rewind it too.
    #[serde(default)]
    pub rng: u64,
    #[serde(default)]
    pub next_id: u64,
    /// Tile rows. `#` blocks movement; other characters are floor unless `tiles` says otherwise.
    pub map: Vec<String>,
    #[serde(default)]
    pub vars: BTreeMap<String, Value>,
    #[serde(default)]
    pub entities: BTreeMap<u64, Entity>,
    /// Map character -> tile type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tiles: BTreeMap<String, TileDef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sprites: BTreeMap<String, Sprite>,
    /// Reusable entity templates: name -> props (may include kind, script, sprite...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prefabs: BTreeMap<String, BTreeMap<String, Value>>,
    /// On-screen text set by scripts (score, lives...), shown over the game.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hud: BTreeMap<String, String>,
    #[serde(skip)]
    pub input: Input,
    /// Pairs of entities overlapping at the end of the last tick (for touch events).
    #[serde(skip)]
    pub contacts: BTreeSet<(u64, u64)>,
    /// Loaded from / saved to `scripts/<name>.rhai` next to world.json.
    #[serde(skip)]
    pub scripts: BTreeMap<String, String>,
    #[serde(skip)]
    pub events: VecDeque<Event>,
    #[serde(skip)]
    pub event_seq: u64,
}

impl World {
    /// An empty world with sensible defaults (the derive's Default leaves tick_rate at 0).
    pub fn blank(map: Vec<String>) -> World {
        World { map, rng: 1, next_id: 1, tick_rate: 8.0, tile_size: 16, ..World::default() }
    }

    pub fn load_dir(dir: &Path) -> Result<World, String> {
        let text = std::fs::read_to_string(dir.join("world.json"))
            .map_err(|e| format!("reading {}: {e}", dir.join("world.json").display()))?;
        let mut w: World = serde_json::from_str(&text).map_err(|e| format!("parsing world.json: {e}"))?;
        let sdir = dir.join("scripts");
        if sdir.is_dir() {
            for entry in std::fs::read_dir(&sdir).map_err(|e| e.to_string())? {
                let p = entry.map_err(|e| e.to_string())?.path();
                if p.extension().and_then(|e| e.to_str()) == Some("rhai") {
                    let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                    let src = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                    w.scripts.insert(name, src);
                }
            }
        }
        w.fix_ids();
        Ok(w)
    }

    /// Makes sure new entities get ids above every existing one.
    pub fn fix_ids(&mut self) {
        let max_id = self.entities.keys().next_back().copied().unwrap_or(0);
        self.next_id = self.next_id.max(max_id + 1);
    }

    pub fn save_dir(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir.join("scripts")).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("world.json"), text + "\n").map_err(|e| e.to_string())?;
        for (name, src) in &self.scripts {
            std::fs::write(dir.join("scripts").join(format!("{name}.rhai")), src).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The whole world as one JSON value, scripts included (export / web player format).
    pub fn to_bundle(&self) -> Value {
        let mut v = serde_json::to_value(self).unwrap_or(Value::Null);
        v["scripts"] = json!(self.scripts);
        v["format"] = json!("forge-world-1");
        v
    }

    pub fn from_bundle(data: &Value) -> Result<World, String> {
        let mut w: World = serde_json::from_value(data.clone()).map_err(|e| format!("not a forge world: {e}"))?;
        if let Some(scripts) = data.get("scripts").and_then(Value::as_object) {
            w.scripts = scripts.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
        }
        w.fix_ids();
        Ok(w)
    }

    pub fn dt(&self) -> f64 {
        1.0 / self.tick_rate.max(0.001)
    }

    pub fn width(&self) -> i64 {
        self.map.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i64
    }

    pub fn height(&self) -> i64 {
        self.map.len() as i64
    }

    fn in_bounds(&self, (x, y): (i64, i64)) -> bool {
        x >= 0 && y >= 0 && x < self.width() && y < self.height()
    }

    /// Out-of-bounds reads as wall.
    pub fn tile(&self, x: i64, y: i64) -> char {
        if x < 0 || y < 0 {
            return '#';
        }
        self.map.get(y as usize).and_then(|r| r.chars().nth(x as usize)).unwrap_or('#')
    }

    pub fn set_tile(&mut self, x: i64, y: i64, c: char) -> bool {
        if x < 0 || y < 0 || y >= self.height() {
            return false;
        }
        let row = &mut self.map[y as usize];
        let mut chars: Vec<char> = row.chars().collect();
        if x as usize >= chars.len() {
            return false;
        }
        chars[x as usize] = c;
        *row = chars.into_iter().collect();
        true
    }

    pub fn tile_def(&self, c: char) -> Option<&TileDef> {
        let mut buf = [0u8; 4];
        self.tiles.get(&*c.encode_utf8(&mut buf))
    }

    pub fn is_solid_char(&self, c: char) -> bool {
        match self.tile_def(c) {
            Some(def) => def.solid,
            None => c == '#',
        }
    }

    pub fn walkable(&self, x: i64, y: i64) -> bool {
        self.in_bounds((x, y)) && !self.is_solid_char(self.tile(x, y))
    }

    /// Clears one-tick input state; called at the end of every tick.
    pub fn end_tick_input(&mut self) {
        self.input.pressed.clear();
        let taps = std::mem::take(&mut self.input.taps);
        for k in taps {
            self.input.down.remove(&k);
        }
    }

    pub fn spawn(&mut self, kind: &str, x: f64, y: f64, mut props: BTreeMap<String, Value>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let script = props.remove("script").and_then(|v| v.as_str().map(String::from));
        self.entities.insert(id, Entity { kind: kind.into(), x, y, script, props });
        id
    }

    /// Creates an entity from a prefab; `extra` props override the prefab's.
    pub fn spawn_prefab(&mut self, name: &str, x: f64, y: f64, extra: BTreeMap<String, Value>) -> Result<u64, String> {
        let mut props = self.prefabs.get(name).cloned().ok_or(format!("no prefab '{name}'"))?;
        props.extend(extra);
        let kind = match props.remove("kind") {
            Some(Value::String(k)) => k,
            _ => name.to_string(),
        };
        props.insert("prefab".into(), json!(name));
        Ok(self.spawn(&kind, x, y, props))
    }

    pub fn emit(&mut self, kind: &str, data: Value) {
        self.event_seq += 1;
        self.events.push_back(Event { seq: self.event_seq, tick: self.tick, kind: kind.into(), data });
        if self.events.len() > EVENT_CAP {
            self.events.pop_front();
        }
    }

    /// splitmix64
    pub fn next_rand(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn entity_json(id: u64, e: &Entity) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), json!(id));
        m.insert("kind".into(), json!(e.kind));
        m.insert("x".into(), num_out(e.x));
        m.insert("y".into(), num_out(e.y));
        if let Some(s) = &e.script {
            m.insert("script".into(), json!(s));
        }
        for (k, v) in &e.props {
            let v = match (k.as_str(), v.as_f64()) {
                ("vx" | "vy", Some(f)) => num_out(f),
                _ => v.clone(),
            };
            m.insert(k.clone(), v);
        }
        Value::Object(m)
    }

    /// Breadth-first distances to `to` over walkable tiles, stopping once `stop` is reached.
    fn bfs(&self, to: (i64, i64), stop: (i64, i64)) -> Vec<u32> {
        let (w, h) = (self.width(), self.height());
        let grid: Vec<Vec<char>> = self.map.iter().map(|r| r.chars().collect()).collect();
        let open = |x: i64, y: i64| {
            x >= 0 && y >= 0 && y < h && grid[y as usize].get(x as usize).is_some_and(|c| !self.is_solid_char(*c))
        };
        let idx = |x: i64, y: i64| (y * w + x) as usize;
        let mut dist = vec![u32::MAX; (w * h).max(0) as usize];
        if !open(to.0, to.1) {
            return dist;
        }
        dist[idx(to.0, to.1)] = 0;
        let mut q = VecDeque::from([to]);
        while let Some((x, y)) = q.pop_front() {
            if (x, y) == stop {
                break;
            }
            let d = dist[idx(x, y)];
            for (dx, dy) in DIRS {
                let (nx, ny) = (x + dx, y + dy);
                if open(nx, ny) && dist[idx(nx, ny)] == u32::MAX {
                    dist[idx(nx, ny)] = d + 1;
                    q.push_back((nx, ny));
                }
            }
        }
        dist
    }

    pub fn path_len(&self, from: (i64, i64), to: (i64, i64)) -> Option<u32> {
        if !self.in_bounds(from) {
            return None;
        }
        let d = self.bfs(to, from)[(from.1 * self.width() + from.0) as usize];
        (d != u32::MAX).then_some(d)
    }

    /// First step of a shortest walkable path from `from` to `to`.
    pub fn next_step(&self, from: (i64, i64), to: (i64, i64)) -> Option<(i64, i64)> {
        if from == to || !self.in_bounds(from) {
            return None;
        }
        let w = self.width();
        let dist = self.bfs(to, from);
        let cur = dist[(from.1 * w + from.0) as usize];
        let mut best: Option<((i64, i64), u32)> = None;
        for (dx, dy) in DIRS {
            let n = (from.0 + dx, from.1 + dy);
            if !self.in_bounds(n) {
                continue;
            }
            let v = dist[(n.1 * w + n.0) as usize];
            if v < cur && best.map_or(true, |(_, b)| v < b) {
                best = Some((n, v));
            }
        }
        best.map(|(n, _)| n)
    }

    /// ASCII render of a window of the world, plus a glyph -> kind legend.
    pub fn ascii(&self, x0: i64, y0: i64, w: i64, h: i64) -> (Vec<String>, BTreeMap<String, String>) {
        let mut grid: Vec<Vec<char>> = (0..h)
            .map(|dy| {
                (0..w)
                    .map(|dx| {
                        let (x, y) = (x0 + dx, y0 + dy);
                        if x < 0 || y < 0 {
                            return ' ';
                        }
                        self.map.get(y as usize).and_then(|r| r.chars().nth(x as usize)).unwrap_or(' ')
                    })
                    .collect()
            })
            .collect();
        let mut ents: Vec<_> = self.entities.iter().collect();
        ents.sort_by_key(|(id, e)| (e.props.get("z").and_then(Value::as_i64).unwrap_or(0), **id));
        let mut legend = BTreeMap::new();
        for (_, e) in ents {
            let g = e.glyph();
            let (cx, cy) = e.cell();
            let (gx, gy) = (cx - x0, cy - y0);
            if gx >= 0 && gy >= 0 && gx < w && gy < h {
                grid[gy as usize][gx as usize] = g;
                legend.insert(g.to_string(), e.kind.clone());
            }
        }
        (grid.into_iter().map(|r| r.into_iter().collect()).collect(), legend)
    }
}

/// Structural diff between two worlds: entity adds/removes/field changes, tiles, vars, scripts.
pub fn diff(a: &World, b: &World) -> Value {
    let mut added = vec![];
    let mut removed = vec![];
    let mut changed = Map::new();
    for (id, eb) in &b.entities {
        let Some(ea) = a.entities.get(id) else {
            added.push(World::entity_json(*id, eb));
            continue;
        };
        let (ja, jb) = (World::entity_json(*id, ea), World::entity_json(*id, eb));
        let (ma, mb) = (ja.as_object().unwrap(), jb.as_object().unwrap());
        let mut ch = Map::new();
        for k in ma.keys().chain(mb.keys()) {
            let (va, vb) = (ma.get(k).unwrap_or(&Value::Null), mb.get(k).unwrap_or(&Value::Null));
            if va != vb {
                ch.insert(k.clone(), json!([va, vb]));
            }
        }
        if !ch.is_empty() {
            changed.insert(id.to_string(), Value::Object(ch));
        }
    }
    for (id, ea) in &a.entities {
        if !b.entities.contains_key(id) {
            removed.push(json!({ "id": id, "kind": ea.kind }));
        }
    }
    let mut tiles = vec![];
    for y in 0..a.height().max(b.height()) {
        for x in 0..a.width().max(b.width()) {
            let (ta, tb) = (a.tile(x, y), b.tile(x, y));
            if ta != tb && tiles.len() < 100 {
                tiles.push(json!({ "x": x, "y": y, "from": ta.to_string(), "to": tb.to_string() }));
            }
        }
    }
    let mut vars = Map::new();
    for k in a.vars.keys().chain(b.vars.keys()) {
        let (va, vb) = (a.vars.get(k).unwrap_or(&Value::Null), b.vars.get(k).unwrap_or(&Value::Null));
        if va != vb {
            vars.insert(k.clone(), json!([va, vb]));
        }
    }
    let scripts: Vec<&String> = a
        .scripts
        .keys()
        .chain(b.scripts.keys())
        .filter(|k| a.scripts.get(*k) != b.scripts.get(*k))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut settings = Map::new();
    if a.physics != b.physics {
        settings.insert("physics".into(), json!([a.physics, b.physics]));
    }
    if a.camera != b.camera {
        settings.insert("camera".into(), json!([a.camera, b.camera]));
    }
    if a.tick_rate != b.tick_rate {
        settings.insert("tick_rate".into(), json!([a.tick_rate, b.tick_rate]));
    }
    json!({
        "ticks": [a.tick, b.tick],
        "added": added,
        "removed": removed,
        "changed": changed,
        "tiles": tiles,
        "vars": vars,
        "scripts_changed": scripts,
        "settings": settings,
    })
}
