//! The command layer: every editor button and every agent action is one of these commands.
//! Editor UI state (selection, tool, tab...) lives here too, so an agent always knows what
//! the user is looking at and can drive the editor itself.

use crate::commands;
use crate::sim::Sim;
use crate::world::{self, Event, Sprite, TileDef, World};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

const UNDO_CAP: usize = 200;

/// What the editor is showing. Changed by the user (clicks) or the agent (`ui` command).
#[derive(Clone, Debug, Serialize)]
pub struct Ui {
    pub rev: u64,
    pub by: String,
    pub selected: Option<u64>,
    pub tool: String,
    pub paint_tile: String,
    pub place: String,
    pub tab: String,
    pub script: Option<String>,
    pub hover: Option<[i64; 2]>,
    /// A selected map cell (clicking a wall or floor), as opposed to an entity.
    pub cell: Option<[i64; 2]>,
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            rev: 0,
            by: "engine".into(),
            selected: None,
            tool: "select".into(),
            paint_tile: "#".into(),
            place: String::new(),
            tab: "activity".into(),
            script: None,
            hover: None,
            cell: None,
        }
    }
}

pub struct App {
    pub sim: Sim,
    pub path: Option<PathBuf>,
    /// Real-time play: the main loop ticks the world `tps` times a second.
    pub playing: bool,
    pub tps: f64,
    pub editor_port: Option<u16>,
    /// Identifies this engine build (the exe's modified time), so the app can spot a stale engine.
    pub build: String,
    /// Started by the Forge app (not an agent), so the app may replace it with a newer build.
    pub managed: bool,
    /// Set by `quit`; the main loop exits after replying.
    pub quit: bool,
    /// The world has changes that aren't saved to its folder yet.
    dirty: bool,
    /// Changes since the last autosave.
    autosave_pending: bool,
    /// Off for tests and throwaway runs (`--no-autosave`).
    pub autosave_enabled: bool,
    last_autosave: Instant,
    /// Unsaved work found from an earlier session, offered to the user.
    recovery: Option<Value>,
    /// > 0 while running the commands of a batch: they share one undo entry.
    in_batch: u32,
    /// > 0 during a preview: nothing is logged or recorded, and the world is rolled back.
    quiet: u32,
    /// Changes the agent suggests for the user to apply or dismiss.
    proposals: Vec<Value>,
    proposal_seq: u64,
    pub ui: Ui,
    /// Each entry is a world state, a label, and the folder that state belongs to.
    undo: Vec<(World, String, Option<PathBuf>)>,
    redo: Vec<(World, String, Option<PathBuf>)>,
    /// Agent -> user messages and map markers, shown in the editor.
    notices: VecDeque<Value>,
    notice_seq: u64,
    /// Recent activity notes, for `look`.
    activity: VecDeque<Value>,
    activity_seq: u64,
    editor_seen: Option<Instant>,
    /// Things the user asked the agent to do, pinned to entities or map cells.
    requests: Vec<Value>,
    request_seq: u64,
    /// The world as of the agent's last `look`, so the next look can report what changed.
    last_look: Option<World>,
    look_cursor: u64,
}

pub fn arg_str<'a>(c: &'a Value, k: &str) -> Option<&'a str> {
    c.get(k).and_then(Value::as_str)
}

pub fn arg_int(c: &Value, k: &str) -> Option<i64> {
    c.get(k).and_then(Value::as_i64)
}

fn need_str<'a>(c: &'a Value, k: &str) -> Result<&'a str, String> {
    arg_str(c, k).ok_or(format!("missing string arg '{k}'"))
}

fn obj_to_props(v: Option<&Value>) -> BTreeMap<String, Value> {
    v.and_then(Value::as_object)
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// Event counts by kind, plus the most recent `max` events (optionally only some kinds).
fn summarize(evs: &[Event], c: &Value, default_max: usize) -> Value {
    let kinds: Option<Vec<&str>> = c
        .get("kinds")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect());
    let max = arg_int(c, "max_events").map_or(default_max, |n| n.max(0) as usize);
    let mut counts = BTreeMap::<&str, u64>::new();
    for e in evs {
        *counts.entry(&e.kind).or_default() += 1;
    }
    let shown: Vec<&Event> = evs
        .iter()
        .filter(|e| kinds.as_ref().map_or(true, |k| k.contains(&e.kind.as_str())))
        .collect();
    let skip = shown.len().saturating_sub(max);
    json!({ "counts": counts, "shown": shown.len() - skip, "recent": shown[skip..] })
}

/// Editor key names: lowercase, arrows as up/down/left/right, " " as space.
fn norm_key(k: &str) -> String {
    match k {
        " " | "Space" => "space".into(),
        "ArrowUp" => "up".into(),
        "ArrowDown" => "down".into(),
        "ArrowLeft" => "left".into(),
        "ArrowRight" => "right".into(),
        other => other.to_lowercase(),
    }
}

fn str_list(c: &Value, k: &str) -> Vec<String> {
    match c.get(k) {
        Some(Value::String(s)) => vec![norm_key(s)],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(norm_key).collect(),
        _ => vec![],
    }
}


/// A short description of a state-changing command for the activity feed, or None for reads.
fn activity_note(c: &Value, cmd: &str, source: &str) -> Option<String> {
    let clip = |s: &str| {
        let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if one.chars().count() > 80 { format!("{}…", one.chars().take(80).collect::<String>()) } else { one }
    };
    let s = |k: &str| arg_str(c, k).unwrap_or("");
    let has = |k: &str| c.get(k).is_some();
    Some(match cmd {
        "new" => "new world".into(),
        "load" => format!("load {}", s("path")),
        "save" => "save".into(),
        "exec" => format!("exec {}", clip(s("code"))),
        "script" if has("code") => format!("edit script '{}'", s("name")),
        "step" => format!("step {} ticks", arg_int(c, "ticks").unwrap_or(1)),
        "snapshot" => format!("snapshot '{}'", s("name")),
        "restore" => format!("restore '{}'", s("name")),
        "trials" => format!("{} trials from '{}'", arg_int(c, "runs").unwrap_or(20), s("from")),
        "play" => "play".into(),
        "pause" => "pause".into(),
        "set" => format!("set #{} {} = {}", c["id"], s("key"), c.get("value").unwrap_or(&Value::Null)),
        "create" => format!("create {} at ({},{})", if has("prefab") { s("prefab") } else { s("kind") }, c["x"], c["y"]),
        "destroy" => format!("destroy #{}", c["id"]),
        "paint" => format!("paint '{}'", s("tile")),
        "resize" => format!("resize to {}x{}", c["width"], c["height"]),
        "tile" if has("char") && c.as_object().is_some_and(|m| m.len() > 2) => format!("define tile '{}'", s("char")),
        "sprite" if has("pixels") || has("delete") => format!("{} sprite '{}'", if has("delete") { "delete" } else { "draw" }, s("name")),
        "prefab" if has("props") || has("delete") => format!("{} prefab '{}'", if has("delete") { "delete" } else { "define" }, s("name")),
        "undo" => "undo".into(),
        "request" => format!("📝 request: {}", s("text")),
        "resolve" => format!("✓ resolved request #{}: {}", c["rid"], s("reply")),
        "import" => "import world".into(),
        "physics" => format!("physics {}", clip(&c.to_string())),
        "camera" => format!("camera {}", clip(&c.to_string())),
        "background" => "change background".into(),
        "goto" => format!("go to level '{}'", s("level")),
        "use_behavior" => format!("🧩 add behavior '{}'{}", s("name"), if has("on") { format!(" to {}", c["on"]) } else { String::new() }),
        "sound" if c.as_object().is_some_and(|m| m.len() > 2) => format!("sound '{}'", s("name")),
        "export_game" => "export playable game".into(),
        "create_world" => format!("create world '{}'", s("name")),
        "duplicate_world" => format!("duplicate world '{}' as '{}'", s("name"), s("to")),
        "rename_world" => format!("rename world '{}' to '{}'", s("name"), s("to")),
        "delete_world" => format!("move world '{}' to trash", s("name")),
        "recover" => "restore unsaved work".into(),
        "set_map" => "replace map".into(),
        "redo" => "redo".into(),
        "say" => format!("💬 {}", s("text")),
        "propose" => format!("💡 suggests: {}", s("title")),
        "apply_proposal" => format!("✓ applied suggestion #{}", c["pid"]),
        "dismiss_proposal" => format!("✗ dismissed suggestion #{}", c["pid"]),
        "point" => format!("📍 {}", if has("label") { s("label") } else { "look here" }),
        "input" if source != "you" => format!("input {}", clip(&c.to_string())),
        "ui" if source != "you" => format!("ui {}", clip(&c.to_string())),
        _ => return None,
    })
}

/// Unsaved work from an earlier session: an autosave newer than the world's own files.
fn find_recovery(world_dir: &std::path::Path) -> Option<Value> {
    let dir = world_dir.join(".autosave");
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("autosave.json")).ok()?).ok()?;
    let saved = std::fs::metadata(world_dir.join("world.json")).and_then(|m| m.modified()).ok()?;
    let auto = std::fs::metadata(dir.join("world.json")).and_then(|m| m.modified()).ok()?;
    if auto <= saved {
        return None;
    }
    Some(json!({ "dir": dir.display().to_string(), "time": meta["time"], "tick": meta["tick"] }))
}

/// A safe world folder name: letters, digits, spaces, dashes and underscores.
fn world_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.starts_with('.') || !n.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_') {
        return Err(format!("'{name}' isn't a valid world name: use letters, numbers, spaces, - and _"));
    }
    Ok(n.to_string())
}

/// Card data for the world browser: size, contents, a small map preview.
fn world_summary(p: &std::path::Path, open: Option<&std::path::Path>) -> Value {
    let name = p.file_name().unwrap().to_string_lossy().into_owned();
    let modified = std::fs::metadata(p.join("world.json"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    match World::load_dir(p) {
        Ok(w) => {
            let mut kinds = BTreeMap::<String, u64>::new();
            for e in w.entities.values() {
                *kinds.entry(e.kind.clone()).or_default() += 1;
            }
            let ents: Vec<Value> = w.entities.values().take(300).map(|e| json!([e.x, e.y, e.kind, e.props.get("color")])).collect();
            // Preview color per tile type: its color, else the most used color in its sprite.
            let tiles: BTreeMap<&String, Option<String>> = w
                .tiles
                .iter()
                .map(|(k, d)| {
                    let from_sprite = d.sprite.as_ref().and_then(|name| {
                        let sp = w.sprites.get(name)?;
                        let mut counts = BTreeMap::<char, usize>::new();
                        for ch in sp.pixels.iter().flat_map(|r| r.chars()).filter(|c| *c != '.' && *c != ' ') {
                            *counts.entry(ch).or_default() += 1;
                        }
                        let top = counts.into_iter().max_by_key(|(_, n)| *n)?.0;
                        sp.palette.get(&top.to_string()).cloned()
                    });
                    (k, d.color.clone().or(from_sprite))
                })
                .collect();
            json!({
                "name": name, "path": p.display().to_string(), "modified": modified,
                "size": [w.width(), w.height()], "entities": w.entities.len(), "kinds": kinds,
                "scripts": w.scripts.len(), "sprites": w.sprites.len(),
                "map": if w.width() * w.height() <= 20000 { json!(w.map) } else { Value::Null },
                "ents": ents, "tile_colors": tiles,
                "open": open == Some(p), "unsaved_work": p.join(".autosave").join("autosave.json").exists(),
            })
        }
        Err(e) => json!({ "name": name, "path": p.display().to_string(), "modified": modified, "error": e }),
    }
}

/// The Forge launcher reopens the last world you had open.
fn remember_last_world(p: &std::path::Path) {
    let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("forge");
    if std::fs::create_dir_all(&dir).is_ok() {
        let full = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let _ = std::fs::write(dir.join("last-world.txt"), full.display().to_string().trim_start_matches(r"\\?\"));
    }
}

/// The behaviour library: ready-made scripts, installed into a world with `use_behavior`.
pub const BEHAVIORS: &[(&str, &str)] = &[
    ("platformer_player", include_str!("behaviors/platformer_player.rhai")),
    ("topdown_player", include_str!("behaviors/topdown_player.rhai")),
    ("patrol", include_str!("behaviors/patrol.rhai")),
    ("chaser", include_str!("behaviors/chaser.rhai")),
    ("shooter", include_str!("behaviors/shooter.rhai")),
    ("projectile", include_str!("behaviors/projectile.rhai")),
    ("health", include_str!("behaviors/health.rhai")),
    ("pickup", include_str!("behaviors/pickup.rhai")),
    ("door", include_str!("behaviors/door.rhai")),
    ("key", include_str!("behaviors/key.rhai")),
    ("button", include_str!("behaviors/button.rhai")),
    ("moving_platform", include_str!("behaviors/moving_platform.rhai")),
    ("goal", include_str!("behaviors/goal.rhai")),
    ("spawner", include_str!("behaviors/spawner.rhai")),
];

/// A behaviour's header comments: what it does, its props, and what it needs.
fn behavior_info(name: &str, src: &str) -> Value {
    let mut about = vec![];
    let (mut props, mut requires) = (String::new(), vec![]);
    for line in src.lines().take_while(|l| l.starts_with("//")) {
        let l = line.trim_start_matches('/').trim();
        if l.starts_with("Behavior:") {
            continue;
        } else if let Some(p) = l.strip_prefix("Props:") {
            props = p.trim().to_string();
        } else if let Some(r) = l.strip_prefix("Requires:") {
            requires = r.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        } else if !l.is_empty() {
            about.push(l.to_string());
        }
    }
    json!({ "name": name, "about": about.join(" "), "props": props, "requires": requires })
}

pub const SOUND_PRESETS: &[&str] = &["jump", "coin", "hit", "explosion", "powerup", "laser", "blip", "stomp", "win"];

/// Ready-made sound recipes, a starting point to tweak.
fn sound_preset(name: &str) -> Option<crate::world::Sound> {
    use crate::world::Sound;
    let s = |wave: &str, freq: f64, slide: f64, dur: f64, vol: f64| Sound { wave: wave.into(), freq, slide, dur, vol, ..Sound::default() };
    Some(match name {
        "jump" => s("square", 260.0, 900.0, 0.18, 0.22),
        "coin" => Sound { arp: vec![0.0, 5.0], arp_speed: 0.07, ..s("square", 988.0, 0.0, 0.25, 0.2) },
        "hit" => s("saw", 330.0, -900.0, 0.22, 0.3),
        "explosion" => s("noise", 120.0, -60.0, 0.6, 0.45),
        "powerup" => Sound { arp: vec![0.0, 4.0, 7.0, 12.0], arp_speed: 0.06, ..s("square", 330.0, 0.0, 0.36, 0.22) },
        "laser" => s("saw", 1400.0, -4000.0, 0.16, 0.2),
        "blip" => s("sine", 660.0, 0.0, 0.07, 0.3),
        "stomp" => s("square", 200.0, -500.0, 0.12, 0.3),
        "win" => Sound { arp: vec![0.0, 4.0, 7.0, 12.0, 7.0, 12.0], arp_speed: 0.1, ..s("triangle", 523.0, 0.0, 0.7, 0.3) },
        _ => return None,
    })
}

/// `inputs` for step/trials: {"0": ["right"], "30": ["right", "space"], "45": []}.
fn input_timeline(c: &Value) -> Result<Option<crate::sim::InputTimeline>, String> {
    let Some(obj) = c.get("inputs") else { return Ok(None) };
    let obj = obj.as_object().ok_or("inputs must be an object: {\"<tick offset>\": [keys held from then on]}")?;
    let mut t = crate::sim::InputTimeline::new();
    for (k, v) in obj {
        let tick: u64 = k.parse().map_err(|_| format!("inputs: '{k}' is not a tick offset (use \"0\", \"30\"...)"))?;
        let keys = match v {
            Value::String(s) => vec![norm_key(s)],
            Value::Array(a) => a.iter().filter_map(Value::as_str).map(norm_key).collect(),
            _ => return Err(format!("inputs[{k}] must be a key or a list of keys")),
        };
        t.insert(tick, keys);
    }
    Ok(Some(t))
}

/// Commands that change the world, and so get an undo entry first.
fn mutates(c: &Value, cmd: &str) -> bool {
    let has = |k: &str| c.get(k).is_some();
    match cmd {
        "new" | "load" | "exec" | "restore" | "create" | "destroy" | "set" | "paint" | "step" | "resize" | "import" | "set_map" | "recover"
        | "create_world" | "physics" | "camera" | "background" | "goto" | "use_behavior" => true,
        "sound" => has("preset") || c.as_object().is_some_and(|m| m.len() > 2) || has("delete"),
        "script" => has("code"),
        "sprite" => has("pixels") || has("delete"),
        "prefab" => has("props") || has("delete"),
        "tile" => has("char") && c.as_object().is_some_and(|m| m.len() > 2),
        _ => false,
    }
}

impl App {
    pub fn new(sim: Sim) -> Self {
        App {
            sim,
            path: None,
            playing: false,
            tps: 8.0,
            editor_port: None,
            build: String::new(),
            managed: false,
            quit: false,
            dirty: false,
            autosave_pending: false,
            autosave_enabled: true,
            last_autosave: Instant::now(),
            recovery: None,
            in_batch: 0,
            quiet: 0,
            proposals: vec![],
            proposal_seq: 0,
            ui: Ui::default(),
            undo: vec![],
            redo: vec![],
            notices: VecDeque::new(),
            notice_seq: 0,
            activity: VecDeque::new(),
            activity_seq: 0,
            editor_seen: None,
            requests: vec![],
            request_seq: 0,
            last_look: None,
            look_cursor: 0,
        }
    }

    pub fn load(&mut self, path: &str) -> Result<Value, String> {
        let p = PathBuf::from(path);
        let w = World::load_dir(&p)?;
        self.tps = w.tick_rate;
        self.sim.replace_world(w);
        self.dirty = false;
        self.autosave_pending = false;
        self.recovery = find_recovery(&p);
        self.sim.level_dir = p.parent().map(|d| d.to_path_buf());
        if self.autosave_enabled {
            remember_last_world(&p);
        }
        self.path = Some(p);
        // The agent's first look reports changes relative to the world as loaded.
        self.last_look = Some(self.sim.world.borrow().clone());
        Ok(self.state())
    }

    /// Where this world's autosave goes: `<world>/.autosave`, or a per-user folder for unsaved worlds.
    fn autosave_dir(&self) -> PathBuf {
        match &self.path {
            Some(p) => p.join(".autosave"),
            None => PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("forge").join("autosave").join("untitled"),
        }
    }

    /// The folder that holds all the worlds: the open world's parent, else `worlds/` found
    /// next to (or above) the forge executable, else `./worlds`.
    fn worlds_dir(&self) -> PathBuf {
        if let Some(parent) = self.path.as_ref().and_then(|p| p.parent()) {
            return parent.to_path_buf();
        }
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.ancestors().map(|d| d.join("worlds")).find(|d| d.is_dir()))
            .unwrap_or_else(|| PathBuf::from("worlds"))
    }

    /// Marks the world as changed (edits, or ticks while playing).
    pub fn touch(&mut self) {
        self.dirty = true;
        self.autosave_pending = true;
    }

    fn write_autosave(&mut self) -> Result<(), String> {
        if !self.autosave_enabled {
            return Ok(());
        }
        let dir = self.autosave_dir();
        let w = self.sim.world.borrow();
        w.save_dir(&dir)?;
        let meta = json!({
            "time": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            "tick": w.tick,
            "world": self.path.as_ref().map(|p| p.display().to_string()),
        });
        std::fs::write(dir.join("autosave.json"), meta.to_string()).map_err(|e| e.to_string())?;
        drop(w);
        self.autosave_pending = false;
        self.last_autosave = Instant::now();
        Ok(())
    }

    /// Called by the main loop: autosaves at most once a minute while there are changes.
    pub fn autosave_tick(&mut self) {
        if self.autosave_pending && self.last_autosave.elapsed().as_secs() >= 60 {
            let _ = self.write_autosave();
        }
    }

    /// Called when the engine exits, so closing without saving never loses work.
    pub fn autosave_now(&mut self) {
        if self.autosave_pending {
            let _ = self.write_autosave();
        }
    }

    /// Call after running ticks: follows a `goto` to the new level's folder.
    pub fn after_ticks(&mut self) {
        let Some(name) = self.sim.level_changed.take() else { return };
        if let Some(dir) = self.sim.level_dir.clone() {
            self.path = Some(dir.join(&name));
        }
        self.tps = self.sim.world.borrow().tick_rate;
        self.ui.selected = None;
        self.ui.cell = None;
        self.ui.rev += 1;
        self.last_look = Some(self.sim.world.borrow().clone());
    }

    /// Seconds since the editor last polled, or None if it never has.
    pub fn editor_idle_secs(&self) -> Option<u64> {
        self.editor_seen.map(|t| t.elapsed().as_secs())
    }

    pub fn editor_url(&self) -> Option<String> {
        self.editor_port.map(|p| format!("http://127.0.0.1:{p}"))
    }

    fn state(&self) -> Value {
        let w = self.sim.world.borrow();
        let mut kinds = BTreeMap::<&str, u64>::new();
        for e in w.entities.values() {
            *kinds.entry(&e.kind).or_default() += 1;
        }
        json!({
            "tick": w.tick,
            "size": [w.width(), w.height()],
            "entities": w.entities.len(),
            "kinds": kinds,
            "scripts": w.scripts.keys().collect::<Vec<_>>(),
            "compile_errors": self.sim.compile_errors,
            "vars": w.vars,
            "snapshots": self.sim.snapshots.keys().collect::<Vec<_>>(),
            "playing": self.playing,
            "editor": self.editor_url(),
            "build": self.build,
            "managed": self.managed,
        })
    }

    fn editor_state(&mut self) -> Value {
        self.editor_seen = Some(Instant::now());
        let w = self.sim.world.borrow();
        let entities: Vec<Value> = w.entities.iter().map(|(id, e)| World::entity_json(*id, e)).collect();
        json!({
            "tick": w.tick,
            "size": [w.width(), w.height()],
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "playing": self.playing,
            "tps": self.tps,
            "entities": entities,
            "scripts": w.scripts,
            "compile_errors": self.sim.compile_errors,
            "vars": w.vars,
            "snapshots": self.sim.snapshots.keys().collect::<Vec<_>>(),
            "tiles": w.tiles,
            "sprites": w.sprites,
            "prefabs": w.prefabs,
            "hud": w.hud,
            "keys_down": w.input.down,
            "ui": self.ui,
            "build": self.build,
            "tick_rate": w.tick_rate,
            "tile_size": w.tile_size,
            "physics": w.physics,
            "camera": w.camera,
            "background": w.background,
            "sounds": w.sounds,
            "unsaved": self.dirty,
            "recovery": self.recovery,
            "notices": self.notices,
            "requests": self.requests,
            "proposals": self.proposals,
            "undo": self.undo.len(),
            "redo": self.redo.len(),
        })
    }

    /// Everything an agent needs to orient itself, in one response. Also reports what the user
    /// changed since the previous look, and any open requests they left.
    fn look(&mut self) -> Value {
        let user_changes: Vec<&Value> = self
            .activity
            .iter()
            .filter(|a| a["seq"].as_u64().unwrap_or(0) > self.look_cursor && a["by"] != "agent")
            .collect();
        let world_diff = self.last_look.as_ref().map(|old| compact_diff(old, &self.sim.world.borrow()));
        let changes = json!({ "by_user_since_last_look": user_changes, "world_diff_since_last_look": world_diff });
        let open_requests: Vec<&Value> = self.requests.iter().filter(|r| r["status"] == "open").collect();
        let mut out = self.look_view();
        out["changes"] = changes;
        out["open_requests"] = json!(open_requests);
        out["open_proposals"] = json!(self.proposals.iter().filter(|p| p["status"] == "open").map(|p| json!({ "pid": p["pid"], "title": p["title"] })).collect::<Vec<_>>());
        self.look_cursor = self.activity_seq;
        self.last_look = Some(self.sim.world.borrow().clone());
        out
    }

    fn look_view(&self) -> Value {
        let w = self.sim.world.borrow();
        let (ww, hh) = (w.width(), w.height());
        // Whole map if it's small; otherwise a window around the user's focus.
        let focus = self
            .ui
            .selected
            .and_then(|id| w.entities.get(&id).map(|e| e.cell()))
            .or(self.ui.hover.map(|h| (h[0], h[1])))
            .unwrap_or((ww / 2, hh / 2));
        let (vw, vh) = (ww.min(80), hh.min(40));
        let vx = (focus.0 - vw / 2).clamp(0, (ww - vw).max(0));
        let vy = (focus.1 - vh / 2).clamp(0, (hh - vh).max(0));
        let (rows, legend) = w.ascii(vx, vy, vw, vh);
        let mut kinds = BTreeMap::<&str, u64>::new();
        for e in w.entities.values() {
            *kinds.entry(&e.kind).or_default() += 1;
        }
        let selected = self.ui.selected.and_then(|id| w.entities.get(&id).map(|e| World::entity_json(id, e)));
        let hover = self.ui.hover.map(|[x, y]| {
            let here: Vec<Value> = w
                .entities
                .iter()
                .filter(|(_, e)| e.cell() == (x, y))
                .map(|(id, e)| World::entity_json(*id, e))
                .collect();
            json!({ "x": x, "y": y, "tile": w.tile(x, y).to_string(), "entities": here })
        });
        let editor_open = self.editor_seen.is_some_and(|t| t.elapsed().as_secs() < 3);
        json!({
            "tick": w.tick,
            "playing": self.playing,
            "tps": self.tps,
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "size": [ww, hh],
            "user": {
                "editor_open": editor_open,
                "selected": selected,
                "selected_cell": self.ui.cell.map(|[x, y]| {
                    let ch = w.tile(x, y);
                    let def = w.tiles.get(&ch.to_string()).cloned();
                    json!({ "x": x, "y": y, "tile": ch.to_string(), "solid": !w.walkable(x, y), "type": def })
                }),
                "hover": hover,
                "tool": self.ui.tool,
                "paint_tile": self.ui.paint_tile,
                "place": self.ui.place,
                "tab": self.ui.tab,
                "open_script": self.ui.script,
            },
            "view": { "origin": [vx, vy], "rows": rows, "legend": legend },
            "kinds": kinds,
            "scripts": w.scripts.keys().collect::<Vec<_>>(),
            "compile_errors": self.sim.compile_errors,
            "tiles": w.tiles,
            "sprites": w.sprites.keys().collect::<Vec<_>>(),
            "prefabs": w.prefabs.keys().collect::<Vec<_>>(),
            "hud": w.hud,
            "vars": w.vars,
            "snapshots": self.sim.snapshots.keys().collect::<Vec<_>>(),
            "tick_rate": w.tick_rate,
            "physics": w.physics,
            "camera": w.camera,
            "background": w.background,
            "sounds": w.sounds.keys().collect::<Vec<_>>(),
            "level": w.level,
            "game": w.game,
            "screen": w.screen,
            "timers": w.timers,
            "tweens": w.tweens.len(),
            "recent_activity": self.activity.iter().rev().take(12).collect::<Vec<_>>(),
            "undo": self.undo.len(),
            "redo": self.redo.len(),
            "unsaved": self.dirty,
            "recovery": self.recovery,
        })
    }

    fn notice(&mut self, kind: &str, mut body: Value, by: &str) {
        self.notice_seq += 1;
        body["seq"] = json!(self.notice_seq);
        body["kind"] = json!(kind);
        body["by"] = json!(by);
        self.notices.push_back(body);
        while self.notices.len() > 30 {
            self.notices.pop_front();
        }
    }

    fn handle(&mut self, c: &Value, source: &str) -> Result<Value, String> {
        let cmd = arg_str(c, "cmd").ok_or("missing 'cmd' (try {\"cmd\":\"help\"})")?;
        match cmd {
            "help" => Ok(commands::help()),
            "propose" => {
                let title = need_str(c, "title")?.to_string();
                let cmds = c.get("commands").cloned().ok_or("missing 'commands'")?;
                // Dry-run it so the user can see what it would change.
                let preview = self.run(&json!({ "cmd": "batch", "commands": cmds, "preview": true }), source);
                if preview["ok"] != json!(true) {
                    return Err(format!("the proposed commands fail: {}", preview["error"].as_str().unwrap_or("see results")));
                }
                self.proposal_seq += 1;
                let p = json!({ "pid": self.proposal_seq, "title": title, "by": source, "commands": cmds, "diff": preview["diff"], "status": "open" });
                self.proposals.push(p.clone());
                Ok(json!({ "proposal": p }))
            }
            "proposals" => {
                let all = c.get("all") == Some(&json!(true));
                Ok(json!({ "proposals": self.proposals.iter().filter(|p| all || p["status"] == "open").collect::<Vec<_>>() }))
            }
            "apply_proposal" | "dismiss_proposal" => {
                let pid = arg_int(c, "pid").ok_or("missing 'pid'")?;
                let i = self.proposals.iter().position(|p| p["pid"] == json!(pid)).ok_or(format!("no proposal {pid}"))?;
                if self.proposals[i]["status"] != "open" {
                    return Err(format!("proposal {pid} is already {}", self.proposals[i]["status"]));
                }
                if cmd == "dismiss_proposal" {
                    self.proposals[i]["status"] = json!("dismissed");
                    return Ok(json!({ "proposal": self.proposals[i] }));
                }
                let cmds = self.proposals[i]["commands"].clone();
                let r = self.run(&json!({ "cmd": "batch", "commands": cmds, "atomic": true }), source);
                if r["ok"] != json!(true) {
                    return Err(format!("applying failed and was rolled back: {}", r["error"].as_str().unwrap_or("")));
                }
                self.proposals[i]["status"] = json!("applied");
                Ok(json!({ "proposal": self.proposals[i], "diff": r["diff"] }))
            }
            "quit" => {
                if !self.managed {
                    return Err("only an engine started by the Forge app can be told to quit".into());
                }
                self.quit = true;
                Ok(json!({ "quitting": true }))
            }
            "look" => Ok(self.look()),

            // --- requests: the user tags things with what they want the agent to do ---
            "request" => {
                let text = need_str(c, "text")?;
                let entity = arg_int(c, "id").map(|id| {
                    let w = self.sim.world.borrow();
                    w.entities.get(&(id as u64)).map(|e| World::entity_json(id as u64, e)).ok_or(format!("no entity {id}"))
                });
                let entity = entity.transpose()?;
                let (x, y) = match &entity {
                    Some(e) => (e["x"].as_f64().unwrap_or(0.0).floor() as i64, e["y"].as_f64().unwrap_or(0.0).floor() as i64),
                    None => (c["x"].as_f64().unwrap_or(0.0).floor() as i64, c["y"].as_f64().unwrap_or(0.0).floor() as i64),
                };
                self.request_seq += 1;
                let r = json!({ "rid": self.request_seq, "text": text, "by": source, "entity": entity, "x": x, "y": y, "status": "open", "reply": null });
                self.requests.push(r.clone());
                Ok(json!({ "request": r }))
            }
            "requests" => {
                let all = c.get("all") == Some(&json!(true));
                let list: Vec<&Value> = self.requests.iter().filter(|r| all || r["status"] == "open").collect();
                Ok(json!({ "requests": list }))
            }
            "resolve" => {
                let rid = arg_int(c, "rid").ok_or("missing 'rid'")?;
                let r = self.requests.iter_mut().find(|r| r["rid"] == json!(rid)).ok_or(format!("no request {rid}"))?;
                r["status"] = json!(arg_str(c, "status").unwrap_or("done"));
                r["reply"] = c.get("reply").cloned().unwrap_or(Value::Null);
                Ok(json!({ "request": r.clone() }))
            }

            // --- import / export ---
            "export" => {
                Ok(json!({ "world": self.sim.world.borrow().to_bundle() }))
            }
            "import" => {
                let data = c.get("world").ok_or("missing 'world' (an exported forge world)")?;
                let w = World::from_bundle(data)?;
                self.tps = w.tick_rate;
                self.sim.replace_world(w);
                self.ui.selected = None;
                self.ui.rev += 1;
                Ok(self.state())
            }
            "set_map" => {
                let rows: Vec<String> = c
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or("missing 'rows' (array of strings)")?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect();
                if rows.is_empty() {
                    return Err("'rows' is empty".into());
                }
                let size = [rows.iter().map(|r| r.chars().count()).max().unwrap_or(0), rows.len()];
                self.sim.world.borrow_mut().map = rows;
                Ok(json!({ "size": size }))
            }
            "editor_state" => Ok(self.editor_state()),
            "state" => Ok(self.state()),

            // --- editor UI, shared between user and agent ---
            "ui" => {
                let ui = &mut self.ui;
                if let Some(v) = c.get("selected") {
                    ui.selected = v.as_u64();
                }
                if let Some(t) = arg_str(c, "tool") {
                    if !["select", "paint", "place", "erase"].contains(&t) {
                        return Err(format!("unknown tool '{t}' (select|paint|place|erase)"));
                    }
                    ui.tool = t.into();
                }
                if let Some(t) = arg_str(c, "tile") {
                    ui.paint_tile = t.chars().next().map(String::from).unwrap_or_default();
                }
                if let Some(p) = arg_str(c, "place") {
                    ui.place = p.into();
                }
                if let Some(t) = arg_str(c, "tab") {
                    ui.tab = t.into();
                }
                if let Some(v) = c.get("script") {
                    ui.script = v.as_str().map(String::from);
                    if ui.script.is_some() && c.get("tab").is_none() {
                        ui.tab = "scripts".into();
                    }
                }
                if let Some(v) = c.get("cell") {
                    ui.cell = v.as_array().and_then(|a| Some([a.first()?.as_i64()?, a.get(1)?.as_i64()?]));
                    if ui.cell.is_some() {
                        ui.selected = None;
                    }
                }
                if let Some(v) = c.get("hover") {
                    ui.hover = v.as_array().and_then(|a| Some([a.first()?.as_i64()?, a.get(1)?.as_i64()?]));
                }
                // Hover alone is too chatty to count as a UI change.
                if c.as_object().is_some_and(|m| m.keys().any(|k| k != "cmd" && k != "hover")) {
                    ui.rev += 1;
                    ui.by = source.into();
                }
                Ok(json!({ "ui": self.ui }))
            }
            "say" => {
                let text = need_str(c, "text")?;
                self.notice("say", json!({ "text": text }), source);
                Ok(json!({ "said": text, "editor_open": self.editor_seen.is_some_and(|t| t.elapsed().as_secs() < 3) }))
            }
            "point" => {
                let (x, y) = match arg_int(c, "id") {
                    Some(id) => {
                        let w = self.sim.world.borrow();
                        let e = w.entities.get(&(id as u64)).ok_or(format!("no entity {id}"))?;
                        e.cell()
                    }
                    None => (
                        c["x"].as_f64().ok_or("need x,y or id")?.floor() as i64,
                        c["y"].as_f64().ok_or("need x,y or id")?.floor() as i64,
                    ),
                };
                let label = arg_str(c, "label").unwrap_or("");
                self.notice("point", json!({ "x": x, "y": y, "label": label }), source);
                Ok(json!({ "pointed": [x, y] }))
            }

            // --- time ---
            "play" => {
                if let Some(t) = c.get("tps").and_then(Value::as_f64) {
                    self.tps = t.clamp(0.5, 120.0);
                }
                self.playing = true;
                Ok(json!({ "playing": true, "tps": self.tps }))
            }
            "pause" => {
                self.playing = false;
                self.sim.world.borrow_mut().input = Default::default();
                Ok(json!({ "playing": false, "tick": self.sim.world.borrow().tick }))
            }
            "step" => {
                let ticks = arg_int(c, "ticks").unwrap_or(1).max(0) as u64;
                let start = self.sim.world.borrow().event_seq;
                let t0 = Instant::now();
                let inputs = input_timeline(c)?;
                let level_before = self.sim.world.borrow().level.clone();
                let (ran, hit) = self.sim.step(ticks, arg_str(c, "until"), inputs.as_ref())?;
                self.after_ticks();
                let level_now = self.sim.world.borrow().level.clone();
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                let evs = self.sim.events_since(start);
                Ok(json!({
                    "tick": self.sim.world.borrow().tick,
                    "ran": ran,
                    "until_hit": hit,
                    "ms": (ms * 10.0).round() / 10.0,
                    "hud": self.sim.world.borrow().hud,
                    "screen": self.sim.world.borrow().screen,
                    "level": if level_now != level_before { json!({ "changed": [level_before, level_now] }) } else { json!(level_now) },
                    "events": summarize(&evs, c, 20),
                }))
            }
            "input" => {
                let mut w = self.sim.world.borrow_mut();
                if c.get("clear") == Some(&json!(true)) {
                    w.input = Default::default();
                }
                for k in str_list(c, "down") {
                    if w.input.down.insert(k.clone()) {
                        w.input.pressed.insert(k);
                    }
                }
                for k in str_list(c, "up") {
                    w.input.down.remove(&k);
                }
                for k in str_list(c, "tap") {
                    w.input.down.insert(k.clone());
                    w.input.pressed.insert(k.clone());
                    w.input.taps.insert(k);
                }
                Ok(json!({ "down": w.input.down, "pressed": w.input.pressed }))
            }
            "undo" | "redo" => {
                let current = self.sim.world.borrow().clone();
                let current_path = self.path.clone();
                let (from, to) = if cmd == "undo" { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
                let (world, label, path) = from.pop().ok_or(format!("nothing to {cmd}"))?;
                to.push((current, label.clone(), current_path));
                self.sim.replace_world(world);
                self.path = path;
                Ok(json!({ cmd: label, "undo": self.undo.len(), "redo": self.redo.len() }))
            }

            // --- entities ---
            "get" => {
                let id = arg_int(c, "id").ok_or("missing 'id'")? as u64;
                let w = self.sim.world.borrow();
                let e = w.entities.get(&id).ok_or(format!("no entity {id}"))?;
                Ok(json!({ "entity": World::entity_json(id, e) }))
            }
            "set" => {
                let id = arg_int(c, "id").ok_or("missing 'id'")? as u64;
                let key = need_str(c, "key")?;
                let v = c.get("value").cloned().unwrap_or(Value::Null);
                let mut w = self.sim.world.borrow_mut();
                let e = w.entities.get_mut(&id).ok_or(format!("no entity {id}"))?;
                match key {
                    "id" => return Err("id is read-only".into()),
                    "x" => e.x = v.as_f64().ok_or("x must be a number")?,
                    "y" => e.y = v.as_f64().ok_or("y must be a number")?,
                    "kind" => e.kind = v.as_str().ok_or("kind must be a string")?.to_string(),
                    "script" => e.script = v.as_str().filter(|s| !s.is_empty()).map(String::from),
                    _ if v.is_null() => {
                        e.props.remove(key);
                    }
                    _ => {
                        e.props.insert(key.to_string(), v);
                    }
                }
                Ok(json!({ "entity": World::entity_json(id, e) }))
            }
            "create" => {
                let (x, y) = (c["x"].as_f64().ok_or("missing 'x'")?, c["y"].as_f64().ok_or("missing 'y'")?);
                let props = obj_to_props(c.get("props"));
                let mut w = self.sim.world.borrow_mut();
                let id = match arg_str(c, "prefab") {
                    Some(p) => w.spawn_prefab(p, x, y, props)?,
                    None => w.spawn(need_str(c, "kind")?, x, y, props),
                };
                Ok(json!({ "id": id, "entity": World::entity_json(id, &w.entities[&id]) }))
            }
            "destroy" => {
                let id = arg_int(c, "id").ok_or("missing 'id'")? as u64;
                if self.sim.world.borrow_mut().entities.remove(&id).is_none() {
                    return Err(format!("no entity {id}"));
                }
                if self.ui.selected == Some(id) {
                    self.ui.selected = None;
                    self.ui.rev += 1;
                }
                Ok(json!({ "destroyed": id }))
            }
            "query" => {
                let near = c.get("near").and_then(Value::as_array).and_then(|a| {
                    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?))
                });
                let limit = arg_int(c, "limit").unwrap_or(50).max(0) as usize;
                let (total, results) = self.sim.query(arg_str(c, "kind"), near, arg_str(c, "where"), limit)?;
                Ok(json!({ "total": total, "results": results }))
            }

            // --- map & assets ---
            "view" => {
                let w = self.sim.world.borrow();
                let (x, y) = (arg_int(c, "x").unwrap_or(0), arg_int(c, "y").unwrap_or(0));
                let ww = arg_int(c, "w").unwrap_or(w.width()).clamp(1, 400);
                let hh = arg_int(c, "h").unwrap_or(w.height()).clamp(1, 400);
                let (rows, legend) = w.ascii(x, y, ww, hh);
                Ok(json!({ "tick": w.tick, "rows": rows, "legend": legend }))
            }
            "paint" => {
                let tile = need_str(c, "tile")?.chars().next().ok_or("empty 'tile'")?;
                let mut cells: Vec<(i64, i64)> = c
                    .get("cells")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|p| Some((p.get(0)?.as_i64()?, p.get(1)?.as_i64()?))).collect())
                    .unwrap_or_default();
                if let Some(r) = c.get("rect").and_then(Value::as_array) {
                    let g = |i: usize| r.get(i).and_then(Value::as_i64).unwrap_or(0);
                    for y in g(1)..g(1) + g(3) {
                        for x in g(0)..g(0) + g(2) {
                            cells.push((x, y));
                        }
                    }
                }
                if cells.is_empty() {
                    return Err("give 'cells' [[x,y],...] or 'rect' [x,y,w,h]".into());
                }
                let mut w = self.sim.world.borrow_mut();
                let painted = cells.into_iter().filter(|&(x, y)| w.set_tile(x, y, tile)).count();
                Ok(json!({ "painted": painted }))
            }
            "resize" => {
                let nw = arg_int(c, "width").ok_or("missing 'width'")?.clamp(1, 400) as usize;
                let nh = arg_int(c, "height").ok_or("missing 'height'")?.clamp(1, 400) as usize;
                let fill = arg_str(c, "fill").and_then(|s| s.chars().next()).unwrap_or('.');
                let mut w = self.sim.world.borrow_mut();
                let mut rows: Vec<String> = w
                    .map
                    .iter()
                    .take(nh)
                    .map(|r| {
                        let mut chars: Vec<char> = r.chars().take(nw).collect();
                        chars.resize(nw, fill);
                        chars.into_iter().collect()
                    })
                    .collect();
                rows.resize(nh, fill.to_string().repeat(nw));
                w.map = rows;
                Ok(json!({ "size": [nw, nh] }))
            }
            "tile" => {
                let mut w = self.sim.world.borrow_mut();
                let Some(ch) = arg_str(c, "char").and_then(|s| s.chars().next()) else {
                    return Ok(json!({ "tiles": w.tiles, "note": "undefined chars are floor, except '#' (solid wall)" }));
                };
                let key = ch.to_string();
                if c.get("delete") == Some(&json!(true)) {
                    w.tiles.remove(&key);
                    return Ok(json!({ "deleted": key }));
                }
                let mut def = w.tiles.get(&key).cloned().unwrap_or(TileDef { solid: ch == '#', ..Default::default() });
                if let Some(v) = c.get("solid") {
                    def.solid = v.as_bool().unwrap_or(false);
                }
                if let Some(v) = c.get("platform") {
                    def.platform = v.as_bool().unwrap_or(false);
                }
                if let Some(v) = c.get("ladder") {
                    def.ladder = v.as_bool().unwrap_or(false);
                }
                for (field, slot) in [("name", &mut def.name), ("color", &mut def.color), ("sprite", &mut def.sprite)] {
                    if let Some(v) = c.get(field) {
                        *slot = v.as_str().map(String::from);
                    }
                }
                w.tiles.insert(key.clone(), def.clone());
                Ok(json!({ "char": key, "tile": def }))
            }
            "sprite" => {
                let mut w = self.sim.world.borrow_mut();
                let Some(name) = arg_str(c, "name") else {
                    return Ok(json!({ "sprites": w.sprites.keys().collect::<Vec<_>>() }));
                };
                if c.get("delete") == Some(&json!(true)) {
                    w.sprites.remove(name).ok_or(format!("no sprite '{name}'"))?;
                    return Ok(json!({ "deleted": name }));
                }
                let Some(rows) = c.get("pixels").and_then(Value::as_array) else {
                    let sp = w.sprites.get(name).ok_or(format!("no sprite '{name}'"))?;
                    return Ok(json!({ "name": name, "sprite": sp }));
                };
                let pixels: Vec<String> = rows.iter().filter_map(Value::as_str).map(String::from).collect();
                let palette: BTreeMap<String, String> = c
                    .get("palette")
                    .and_then(Value::as_object)
                    .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
                    .or_else(|| w.sprites.get(name).map(|s| s.palette.clone()))
                    .unwrap_or_default();
                let missing: Vec<char> = pixels
                    .iter()
                    .flat_map(|r| r.chars())
                    .filter(|ch| *ch != '.' && *ch != ' ' && !palette.contains_key(&ch.to_string()))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if !missing.is_empty() {
                    return Err(format!("palette has no color for {missing:?}"));
                }
                let frames: Vec<Vec<String>> = match c.get("frames") {
                    Some(Value::Array(fs)) => fs
                        .iter()
                        .map(|f| f.as_array().map(|rows| rows.iter().filter_map(Value::as_str).map(String::from).collect()).ok_or("frames: each frame is an array of rows"))
                        .collect::<Result<_, _>>()?,
                    Some(_) => return Err("frames must be an array of frames (each an array of rows)".into()),
                    None => vec![],
                };
                let missing_in_frames: Vec<char> = frames
                    .iter()
                    .flatten()
                    .flat_map(|r| r.chars())
                    .filter(|ch| *ch != '.' && *ch != ' ' && !palette.contains_key(&ch.to_string()))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if !missing_in_frames.is_empty() {
                    return Err(format!("frames: palette has no color for {missing_in_frames:?}"));
                }
                let fps = c["fps"].as_f64().unwrap_or(8.0).clamp(0.5, 60.0);
                let size = [pixels.iter().map(|r| r.chars().count()).max().unwrap_or(0), pixels.len()];
                let count = 1 + frames.len();
                w.sprites.insert(name.into(), Sprite { palette, pixels, frames, fps });
                Ok(json!({ "name": name, "size": size, "frames": count }))
            }
            "prefab" => {
                let mut w = self.sim.world.borrow_mut();
                let Some(name) = arg_str(c, "name") else {
                    return Ok(json!({ "prefabs": w.prefabs }));
                };
                if c.get("delete") == Some(&json!(true)) {
                    w.prefabs.remove(name).ok_or(format!("no prefab '{name}'"))?;
                    return Ok(json!({ "deleted": name }));
                }
                match c.get("props") {
                    Some(p) => {
                        w.prefabs.insert(name.into(), obj_to_props(Some(p)));
                        Ok(json!({ "name": name, "prefab": w.prefabs[name] }))
                    }
                    None => Ok(json!({ "name": name, "prefab": w.prefabs.get(name).ok_or(format!("no prefab '{name}'"))? })),
                }
            }

            // --- world files, scripts, experiments ---
            "load" => self.load(need_str(c, "path")?),
            "physics" => {
                let mut w = self.sim.world.borrow_mut();
                match arg_str(c, "preset") {
                    Some("platformer") => {
                        w.physics.gravity = 60.0;
                        w.physics.max_fall = 22.0;
                        w.tick_rate = 60.0;
                    }
                    Some("topdown") => {
                        w.physics.gravity = 0.0;
                        w.tick_rate = 60.0;
                    }
                    Some("grid") => {
                        w.physics.gravity = 0.0;
                        w.tick_rate = 8.0;
                    }
                    Some(other) => return Err(format!("unknown preset '{other}' (platformer | topdown | grid)")),
                    None => {}
                }
                if let Some(g) = c["gravity"].as_f64() {
                    w.physics.gravity = g;
                }
                if let Some(m) = c["max_fall"].as_f64() {
                    w.physics.max_fall = m;
                }
                if let Some(r) = c["tick_rate"].as_f64() {
                    w.tick_rate = r.clamp(1.0, 240.0);
                }
                self.tps = w.tick_rate;
                Ok(json!({ "physics": w.physics, "tick_rate": w.tick_rate, "jump_3_tiles_speed": crate::physics::jump_speed(w.physics.gravity, 3.0) }))
            }
            "camera" => {
                let mut w = self.sim.world.borrow_mut();
                let cam = &mut w.camera;
                if let Some(v) = c.get("follow") {
                    cam.follow = v.as_u64();
                }
                for (k, slot) in [("x", &mut cam.x), ("y", &mut cam.y), ("zoom", &mut cam.zoom), ("lerp", &mut cam.lerp), ("shake", &mut cam.shake)] {
                    if let Some(v) = c[k].as_f64() {
                        *slot = v;
                    }
                }
                if let Some(v) = c["view"].as_array() {
                    cam.view = [v.first().and_then(Value::as_f64).unwrap_or(0.0), v.get(1).and_then(Value::as_f64).unwrap_or(0.0)];
                }
                if let Some(b) = c["bounds"].as_bool() {
                    cam.bounds = b;
                }
                if let Some(t) = c["tile_size"].as_u64() {
                    w.tile_size = t.clamp(4, 128) as u32;
                }
                crate::physics::update_camera(&mut w);
                Ok(json!({ "camera": w.camera, "tile_size": w.tile_size }))
            }
            "background" => {
                let mut w = self.sim.world.borrow_mut();
                if c.get("clear") == Some(&json!(true)) {
                    w.background = Default::default();
                }
                match c.get("sky") {
                    Some(Value::String(s)) => w.background.sky = vec![s.clone()],
                    Some(Value::Array(a)) => w.background.sky = a.iter().filter_map(Value::as_str).map(String::from).collect(),
                    Some(Value::Null) => w.background.sky.clear(),
                    _ => {}
                }
                if let Some(layers) = c.get("layers") {
                    let layers: Vec<crate::world::Layer> = serde_json::from_value(layers.clone())
                        .map_err(|e| format!("layers: {e} (each layer: {{sprite, parallax, y, height, repeat}})"))?;
                    for l in &layers {
                        if !w.sprites.contains_key(&l.sprite) {
                            return Err(format!("layers: no sprite '{}' (draw it first with the sprite command)", l.sprite));
                        }
                    }
                    w.background.layers = layers;
                }
                Ok(json!({ "background": w.background }))
            }
            "sound" => {
                let mut w = self.sim.world.borrow_mut();
                let Some(name) = arg_str(c, "name") else {
                    return Ok(json!({ "sounds": w.sounds, "presets": SOUND_PRESETS }));
                };
                if c.get("delete") == Some(&json!(true)) {
                    w.sounds.remove(name).ok_or(format!("no sound '{name}'"))?;
                    return Ok(json!({ "deleted": name }));
                }
                let mut snd = match arg_str(c, "preset") {
                    Some(p) => sound_preset(p).ok_or(format!("unknown preset '{p}' ({})", SOUND_PRESETS.join(" | ")))?,
                    None => w.sounds.get(name).cloned().unwrap_or_default(),
                };
                if let Some(v) = arg_str(c, "wave") {
                    if !["square", "sine", "triangle", "saw", "noise"].contains(&v) {
                        return Err(format!("unknown wave '{v}' (square | sine | triangle | saw | noise)"));
                    }
                    snd.wave = v.into();
                }
                for (k, slot) in [("freq", &mut snd.freq), ("slide", &mut snd.slide), ("dur", &mut snd.dur), ("vol", &mut snd.vol),
                                  ("attack", &mut snd.attack), ("vibrato", &mut snd.vibrato), ("vibrato_rate", &mut snd.vibrato_rate), ("arp_speed", &mut snd.arp_speed)] {
                    if let Some(v) = c[k].as_f64() {
                        *slot = v;
                    }
                }
                if let Some(a) = c["arp"].as_array() {
                    snd.arp = a.iter().filter_map(Value::as_f64).collect();
                }
                snd.dur = snd.dur.clamp(0.01, 5.0);
                snd.vol = snd.vol.clamp(0.0, 1.0);
                w.sounds.insert(name.into(), snd.clone());
                Ok(json!({ "name": name, "sound": snd }))
            }
            "goto" => {
                let name = need_str(c, "level")?;
                let mut next = self.sim.load_level(name)?;
                let game = self.sim.world.borrow().game.clone();
                if c.get("keep_game") != Some(&json!(false)) {
                    next.game.extend(game);
                }
                next.level = name.to_string();
                self.sim.replace_world(next);
                self.sim.level_changed = Some(name.to_string());
                self.after_ticks();
                Ok(self.state())
            }
            "behaviors" => {
                let list: Vec<Value> = BEHAVIORS.iter().map(|(n, src)| behavior_info(n, src)).collect();
                Ok(json!({
                    "behaviors": list,
                    "how": "use_behavior installs one (and what it requires) as a script; attach it with the entity prop behaviors: [names] (several can run together) or as its script",
                }))
            }
            "use_behavior" => {
                let name = need_str(c, "name")?;
                let overwrite = c.get("overwrite") == Some(&json!(true));
                // The behaviour plus everything it requires.
                let mut todo = vec![name.to_string()];
                let mut order = vec![];
                while let Some(n) = todo.pop() {
                    if order.contains(&n) {
                        continue;
                    }
                    let (_, src) = BEHAVIORS.iter().find(|(b, _)| *b == n).ok_or_else(|| {
                        format!("no behavior '{n}' (available: {})", BEHAVIORS.iter().map(|(b, _)| *b).collect::<Vec<_>>().join(", "))
                    })?;
                    todo.extend(behavior_info(&n, src)["requires"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from));
                    order.push(n);
                }
                let (mut installed, mut kept) = (vec![], vec![]);
                for n in &order {
                    let src = BEHAVIORS.iter().find(|(b, _)| b == n).unwrap().1;
                    if !overwrite && self.sim.world.borrow().scripts.contains_key(n.as_str()) {
                        kept.push(n.clone());
                        continue;
                    }
                    self.sim.set_script(n, src).map_err(|e| format!("behavior '{n}' failed to compile: {e}"))?;
                    installed.push(n.clone());
                }
                let mut attached = vec![];
                let ids: Vec<u64> = match c.get("on") {
                    Some(Value::Array(a)) => a.iter().filter_map(Value::as_u64).collect(),
                    Some(v) => v.as_u64().into_iter().collect(),
                    None => vec![],
                };
                {
                    let mut w = self.sim.world.borrow_mut();
                    for id in ids {
                        let e = w.entities.get_mut(&id).ok_or(format!("no entity {id}"))?;
                        let mut list: Vec<Value> = e.props.get("behaviors").and_then(Value::as_array).cloned().unwrap_or_default();
                        if !list.iter().any(|v| v == name) {
                            list.push(json!(name));
                        }
                        e.props.insert("behaviors".into(), json!(list));
                        attached.push(id);
                    }
                }
                Ok(json!({ "installed": installed, "kept_existing": kept, "attached_to": attached }))
            }
            "screenshot" => {
                let w = self.sim.world.borrow();
                let camera = match arg_str(c, "view") {
                    Some("map") => false,
                    Some("camera") | None => true,
                    Some(v) => return Err(format!("view must be 'camera' or 'map', not '{v}'")),
                };
                let area = c.get("area").and_then(Value::as_array).and_then(|a| {
                    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?, a.get(3)?.as_f64()?])
                });
                let shot = crate::raster::screenshot(&w, camera, area, c["scale"].as_f64());
                let mut out = json!({
                    "width": shot.width, "height": shot.height, "area": shot.area, "tick": w.tick, "hud": w.hud,
                    "screen": w.screen,
                    "note": "HUD and glyph text aren't drawn in screenshots; hud is listed here",
                });
                if let Some(p) = arg_str(c, "save") {
                    std::fs::write(p, &shot.png).map_err(|e| format!("saving {p}: {e}"))?;
                    out["saved"] = json!(p);
                }
                if c.get("data") != Some(&json!(false)) {
                    out["image"] = json!({ "mime": "image/png", "base64": crate::raster::base64(&shot.png) });
                }
                Ok(out)
            }
            "export_game" => {
                let title = arg_str(c, "title")
                    .map(String::from)
                    .or_else(|| self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "Forge game".into());
                let dir = crate::viewer::EXPORTS_DIR.get().cloned().unwrap_or_else(|| PathBuf::from("exports"));
                let levels = crate::export::linked_levels(&self.sim);
                let file = crate::export::export(&self.sim.world.borrow(), &levels, &title, &dir)?;
                let name = file.file_name().unwrap().to_string_lossy().into_owned();
                let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                Ok(json!({
                    "file": file.display().to_string(),
                    "size_kb": size / 1024,
                    "url": self.editor_url().map(|u| format!("{u}/exports/{}", name.replace(' ', "%20"))),
                    "levels": levels.keys().collect::<Vec<_>>(),
                    "note": "one self-contained .html file: double-click to play, or send it to anyone",
                }))
            }

            // --- the world browser ---
            "worlds" => {
                let dir = self.worlds_dir();
                let mut list = vec![];
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        let name = p.file_name().unwrap().to_string_lossy().into_owned();
                        if name.starts_with('.') || !p.join("world.json").exists() {
                            continue;
                        }
                        list.push(world_summary(&p, self.path.as_deref()));
                    }
                }
                list.sort_by(|a, b| b["modified"].as_u64().cmp(&a["modified"].as_u64()));
                Ok(json!({ "dir": dir.display().to_string(), "worlds": list }))
            }
            "create_world" => {
                let name = world_name(need_str(c, "name")?)?;
                let dest = self.worlds_dir().join(&name);
                if dest.exists() {
                    return Err(format!("a world called '{name}' already exists"));
                }
                let w = match arg_str(c, "copy_of") {
                    Some(src) => World::load_dir(&self.worlds_dir().join(world_name(src)?))?,
                    None => {
                        let (w, h) = (arg_int(c, "width").unwrap_or(24).clamp(3, 400) as usize, arg_int(c, "height").unwrap_or(14).clamp(3, 400) as usize);
                        let wall = "#".repeat(w);
                        let mid = format!("#{}#", ".".repeat(w - 2));
                        let map = (0..h).map(|y| if y == 0 || y == h - 1 { wall.clone() } else { mid.clone() }).collect();
                        World::blank(map)
                    }
                };
                w.save_dir(&dest)?;
                self.load(&dest.display().to_string())
            }
            "duplicate_world" => {
                let src = self.worlds_dir().join(world_name(need_str(c, "name")?)?);
                let to = world_name(need_str(c, "to")?)?;
                let dest = self.worlds_dir().join(&to);
                if dest.exists() {
                    return Err(format!("a world called '{to}' already exists"));
                }
                World::load_dir(&src)?.save_dir(&dest)?;
                Ok(json!({ "created": to }))
            }
            "rename_world" => {
                let from = world_name(need_str(c, "name")?)?;
                let to = world_name(need_str(c, "to")?)?;
                let (src, dest) = (self.worlds_dir().join(&from), self.worlds_dir().join(&to));
                if dest.exists() {
                    return Err(format!("a world called '{to}' already exists"));
                }
                std::fs::rename(&src, &dest).map_err(|e| format!("couldn't rename: {e}"))?;
                if self.path.as_deref() == Some(src.as_path()) {
                    self.path = Some(dest.clone());
                    if self.autosave_enabled {
                        remember_last_world(&dest);
                    }
                }
                Ok(json!({ "renamed": [from, to] }))
            }
            "delete_world" => {
                let name = world_name(need_str(c, "name")?)?;
                let src = self.worlds_dir().join(&name);
                if self.path.as_deref() == Some(src.as_path()) {
                    return Err("that world is open; open another world first".into());
                }
                // Never really deleted: moved to worlds/.trash so it can be recovered by hand.
                let trash = self.worlds_dir().join(".trash");
                std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let dest = trash.join(format!("{name}-{stamp}"));
                std::fs::rename(&src, &dest).map_err(|e| format!("couldn't move to trash: {e}"))?;
                Ok(json!({ "trashed": name, "to": dest.display().to_string() }))
            }
            "recover" => {
                let dir = self.recovery.as_ref().and_then(|r| r["dir"].as_str()).map(PathBuf::from).ok_or("no unsaved work to recover")?;
                let w = World::load_dir(&dir)?;
                self.sim.replace_world(w);
                self.recovery = None;
                self.touch();
                Ok(json!({ "recovered": dir.display().to_string(), "tick": self.sim.world.borrow().tick }))
            }
            "discard_recovery" => {
                let r = self.recovery.take().ok_or("no unsaved work to discard")?;
                if let Some(d) = r["dir"].as_str() {
                    let _ = std::fs::remove_dir_all(d);
                }
                Ok(json!({ "discarded": r }))
            }
            "new" => {
                let map: Vec<String> = match c.get("map").and_then(Value::as_array) {
                    Some(rows) => rows.iter().filter_map(Value::as_str).map(String::from).collect(),
                    None => {
                        let w = arg_int(c, "width").unwrap_or(20).clamp(3, 400) as usize;
                        let h = arg_int(c, "height").unwrap_or(10).clamp(3, 400) as usize;
                        let wall = "#".repeat(w);
                        let mid = format!("#{}#", ".".repeat(w - 2));
                        (0..h).map(|y| if y == 0 || y == h - 1 { wall.clone() } else { mid.clone() }).collect()
                    }
                };
                let seed = arg_int(c, "seed").unwrap_or(1) as u64;
                self.sim.replace_world(World { rng: seed, ..World::blank(map) });
                self.tps = 8.0;
                self.path = arg_str(c, "path").map(PathBuf::from);
                self.ui.selected = None;
                self.ui.rev += 1;
                Ok(self.state())
            }
            "save" => {
                let p = arg_str(c, "path").map(PathBuf::from).or(self.path.clone()).ok_or("no path: give 'path'")?;
                self.sim.world.borrow().save_dir(&p)?;
                self.path = Some(p.clone());
                self.dirty = false;
                self.autosave_pending = false;
                self.recovery = None;
                let _ = std::fs::remove_dir_all(p.join(".autosave"));
                Ok(json!({ "saved": p.display().to_string() }))
            }
            "script" => match (arg_str(c, "name"), arg_str(c, "code")) {
                (None, _) => Ok(json!({ "scripts": self.sim.world.borrow().scripts.keys().collect::<Vec<_>>() })),
                (Some(n), None) => {
                    let w = self.sim.world.borrow();
                    let src = w.scripts.get(n).ok_or(format!("no script '{n}'"))?;
                    Ok(json!({ "name": n, "code": src }))
                }
                (Some(n), Some(code)) => {
                    let fns = self.sim.set_script(n, code)?;
                    Ok(json!({ "name": n, "functions": fns }))
                }
            },
            "exec" => {
                let start = self.sim.world.borrow().event_seq;
                let result = self.sim.exec(need_str(c, "code")?)?;
                let evs = self.sim.events_since(start);
                Ok(json!({ "result": result, "events": summarize(&evs, c, 20) }))
            }
            "snapshot" => {
                let name = need_str(c, "name")?;
                let w = self.sim.world.borrow().clone();
                let tick = w.tick;
                self.sim.snapshots.insert(name.into(), w);
                Ok(json!({ "snapshot": name, "tick": tick }))
            }
            "restore" => {
                let name = need_str(c, "name")?;
                let w = self.sim.snapshots.get(name).cloned().ok_or(format!("no snapshot '{name}'"))?;
                self.sim.replace_world(w);
                Ok(json!({ "restored": name, "tick": self.sim.world.borrow().tick }))
            }
            "diff" => {
                let from = need_str(c, "from")?;
                let a = self.sim.snapshots.get(from).ok_or(format!("no snapshot '{from}'"))?;
                let cur = self.sim.world.borrow();
                let b = match arg_str(c, "to") {
                    Some(t) => self.sim.snapshots.get(t).ok_or(format!("no snapshot '{t}'"))?,
                    None => &*cur,
                };
                Ok(world::diff(a, b))
            }
            "trials" => {
                let t0 = Instant::now();
                let inputs = input_timeline(c)?;
                let mut r = self.sim.trials(
                    need_str(c, "from")?,
                    arg_int(c, "runs").unwrap_or(20).max(1) as u64,
                    arg_int(c, "ticks").unwrap_or(500).max(0) as u64,
                    arg_str(c, "until"),
                    need_str(c, "metric")?,
                    inputs.as_ref(),
                )?;
                if c.get("detail") != Some(&json!(true)) {
                    r.as_object_mut().unwrap().remove("runs");
                }
                r["ms"] = json!((t0.elapsed().as_secs_f64() * 1000.0).round());
                Ok(r)
            }
            "events" => {
                let since = arg_int(c, "since").unwrap_or(0).max(0) as u64;
                let evs = self.sim.events_since(since);
                Ok(summarize(&evs, c, 50))
            }
            _ => Err(format!("'{cmd}' is in the catalogue but not implemented")),
        }
    }

    /// Runs one command and wraps the result as `{"ok":..,"cmd":..,...}`. Changes get an undo
    /// entry and are logged to the editor's activity feed, tagged with `source`.
    pub fn run(&mut self, c: &Value, source: &str) -> Value {
        let name = c.get("cmd").cloned().unwrap_or(Value::Null);
        if let Err(e) = commands::validate(c) {
            return json!({ "ok": false, "cmd": name, "error": e });
        }
        let cmd = name.as_str().unwrap_or("?").to_string();
        if cmd == "batch" {
            return self.run_batch(c, source);
        }
        let note = if self.quiet > 0 { None } else { activity_note(c, &cmd, source).map(|d| format!("{source} · {d}")) };
        let undoable = mutates(c, &cmd) && self.in_batch == 0;
        if undoable {
            let snap = self.sim.world.borrow().clone();
            self.undo.push((snap, format!("{source}: {}", note.as_deref().unwrap_or(&cmd)), self.path.clone()));
            if self.undo.len() > UNDO_CAP {
                self.undo.remove(0);
            }
        }
        // For step, log the note first so it appears before the ticks it produced.
        if let (Some(n), "step") = (&note, cmd.as_str()) {
            self.sim.record(Some(n), vec![], false);
        }
        let result = self.handle(c, source);
        match &result {
            Ok(_) if undoable => {
                self.redo.clear();
                self.touch();
            }
            Ok(_) if mutates(c, &cmd) && self.quiet == 0 => self.touch(),
            Err(_) if undoable => {
                self.undo.pop();
            }
            _ => {}
        }
        if let Some(n) = &note {
            let n = match &result {
                Ok(r) if cmd == "trials" => format!("{n} → {}", r["summary"]),
                Ok(r) if cmd == "undo" || cmd == "redo" => format!("{n}: {}", r[cmd.as_str()].as_str().unwrap_or("")),
                Ok(_) => n.clone(),
                Err(e) => format!("{n} ✗ {e}"),
            };
            if cmd != "step" {
                let jump = matches!(cmd.as_str(), "new" | "load" | "restore" | "undo" | "redo");
                self.sim.record(Some(&n), vec![], jump);
            }
            let tick = self.sim.world.borrow().tick;
            self.activity_seq += 1;
            self.activity.push_back(json!({ "seq": self.activity_seq, "tick": tick, "by": source, "note": n }));
            while self.activity.len() > 50 {
                self.activity.pop_front();
            }
        }
        match result {
            Ok(body) => {
                let mut r = json!({ "ok": true, "cmd": name });
                if let Value::Object(m) = body {
                    r.as_object_mut().unwrap().extend(m);
                }
                r
            }
            Err(e) => json!({ "ok": false, "cmd": name, "error": e }),
        }
    }

    /// Runs a batch: one undo entry for the lot; `atomic` rolls everything back if any command
    /// fails; `preview` always rolls back and reports what would have changed.
    fn run_batch(&mut self, c: &Value, source: &str) -> Value {
        let cmds = c["commands"].as_array().cloned().unwrap_or_default();
        let atomic = c.get("atomic") == Some(&json!(true));
        let preview = c.get("preview") == Some(&json!(true));
        let before = self.sim.world.borrow().clone();
        let path_before = self.path.clone();
        let recording = self.sim.recording;
        if preview {
            self.quiet += 1;
            self.sim.recording = false;
        }
        self.in_batch += 1;
        let mut results = vec![];
        let mut failed_at = None;
        for (i, sub) in cmds.iter().enumerate() {
            let r = if sub.get("cmd") == Some(&json!("batch")) {
                json!({ "ok": false, "cmd": "batch", "error": "batches can't be nested; put all the commands in one batch" })
            } else {
                self.run(sub, source)
            };
            let ok = r["ok"] == json!(true);
            results.push(r);
            if !ok {
                failed_at = Some(i);
                if atomic || preview {
                    break;
                }
            }
        }
        self.in_batch -= 1;
        let diff = compact_diff(&before, &self.sim.world.borrow());
        let changed = diff.as_object().is_some_and(|m| m.keys().any(|k| k != "ticks"));
        let rolled_back = preview || (atomic && failed_at.is_some());
        if rolled_back {
            self.sim.replace_world(before);
        } else if changed && self.in_batch == 0 {
            self.undo.push((before, format!("{source}: batch of {} commands", cmds.len()), path_before));
            if self.undo.len() > UNDO_CAP {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.touch();
        }
        if preview {
            self.quiet -= 1;
            self.sim.recording = recording;
        } else if self.quiet == 0 && changed {
            let note = format!("{source} · batch of {} commands{}", cmds.len(), if rolled_back { " ✗ rolled back" } else { "" });
            self.sim.record(Some(&note), vec![], false);
            let tick = self.sim.world.borrow().tick;
            self.activity_seq += 1;
            self.activity.push_back(json!({ "seq": self.activity_seq, "tick": tick, "by": source, "note": note }));
        }
        let mut out = json!({
            "ok": failed_at.is_none(),
            "cmd": "batch",
            "results": results,
            "diff": diff,
            "rolled_back": rolled_back,
        });
        if let Some(i) = failed_at {
            out["failed_at"] = json!(i);
            out["error"] = json!(format!("command {i} ({}) failed: {}", results[i]["cmd"], results[i]["error"].as_str().unwrap_or("")));
        }
        out
    }
}

/// A world diff with the empty sections left out.
fn compact_diff(a: &World, b: &World) -> Value {
    let d = world::diff(a, b);
    let mut m = serde_json::Map::new();
    for (k, v) in d.as_object().unwrap() {
        let empty = v.as_array().is_some_and(|a| a.is_empty()) || v.as_object().is_some_and(|o| o.is_empty());
        if !empty {
            m.insert(k.clone(), v.clone());
        }
    }
    Value::Object(m)
}
