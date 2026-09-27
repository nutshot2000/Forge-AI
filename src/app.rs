//! The command layer: every editor button and every agent action is one of these commands.
//! Editor UI state (selection, tool, tab...) lives here too, so an agent always knows what
//! the user is looking at and can drive the editor itself.

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
    pub ui: Ui,
    undo: Vec<(World, String)>,
    redo: Vec<(World, String)>,
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

pub const HELP: &str = r#"{
  "protocol": "one JSON object per line in, one per line out. Every command has a 'cmd' field.",
  "start_here": "Call look first: it returns the map, what the user has selected/hovered, what they changed since your last look, open requests they pinned on things, assets, HUD and recent activity. The user watches and edits the same world live in the editor; use say to talk to them there, point to show them a spot, and resolve requests with a reply when done.",
  "commands": {
    "look": {"_": "everything at once: map view, user's selection + hover, tool/tab, counts, scripts, assets, hud, recent activity, undo depth"},
    "help": {},
    "new": {"map": "opt array of tile rows", "width": "or: walled empty room, default 20", "height": "default 10", "seed": "rng seed", "path": "opt folder for later saves"},
    "load": {"path": "world folder (world.json + scripts/*.rhai)"},
    "save": {"path": "optional; defaults to the loaded folder"},
    "state": {"_": "tick, size, counts by kind, scripts, vars, snapshots, compile errors"},
    "get": {"id": "entity id"},
    "query": {"kind": "opt", "near": "opt [x,y,r]", "where": "opt Rhai expr over e, e.g. e.hp < 5", "limit": "default 50"},
    "view": {"x": 0, "y": 0, "w": "default map width", "h": "default map height"},
    "set": {"id": "entity", "key": "x, y, kind, script or any prop (sprite, color, glyph, hp...)", "value": "any JSON; null removes a prop"},
    "create": {"kind": "kind (or omit when using prefab)", "prefab": "opt prefab name", "x": 0, "y": 0, "props": "opt object"},
    "destroy": {"id": "entity"},
    "paint": {"tile": "one map char", "cells": "[[x,y], ...]", "rect": "or [x,y,w,h]"},
    "resize": {"width": "new width", "height": "new height", "fill": "char for new cells, default '.'"},
    "tile": {"char": "map char; omit to list", "name": "opt", "solid": "bool", "color": "css color", "sprite": "opt sprite name", "delete": "opt bool"},
    "sprite": {"name": "omit to list", "pixels": "rows of palette chars ('.' = transparent), e.g. 8x8", "palette": "{char: css color}", "delete": "opt bool"},
    "prefab": {"name": "omit to list", "props": "entity template: kind, script, sprite, hp...", "delete": "opt bool"},
    "script": {"name": "script name", "code": "opt: new source (compile-checked). Omit to read it; omit name to list"},
    "exec": {"code": "Rhai code run against the live world; variables persist between execs"},
    "step": {"ticks": "default 1", "until": "opt Rhai expr, stops when true", "kinds": "opt event kinds to show", "max_events": "default 20"},
    "play": {"tps": "opt ticks per second (default 8): run in real time"},
    "pause": {},
    "input": {"tap": "key(s) pressed for one tick", "down": "key(s) to hold", "up": "key(s) to release", "clear": "release all"},
    "undo": {}, "redo": {},
    "snapshot": {"name": "save the full world state in memory"},
    "restore": {"name": "rewind to a snapshot"},
    "diff": {"from": "snapshot", "to": "opt snapshot; default current world"},
    "trials": {"from": "snapshot", "runs": 20, "ticks": 500, "until": "opt expr", "metric": "Rhai expr scored at end of each run", "detail": "opt bool"},
    "events": {"since": "opt event seq", "kinds": "opt", "max_events": "default 50"},
    "ui": {"selected": "entity id or null", "cell": "[x,y] to select a map tile, or null", "tool": "select|paint|place|erase", "tile": "paint char", "place": "prefab/kind to place", "tab": "activity|scripts|assets|console", "script": "script to open"},
    "say": {"text": "message shown to the user in the editor"},
    "request": {"text": "what should happen", "id": "entity it's about", "x": "or a cell", "y": ""},
    "requests": {"all": "opt bool: include resolved ones"},
    "resolve": {"rid": "request id", "reply": "what you did (shown to the user)", "status": "opt: done (default) or declined"},
    "export": {"_": "the whole world as one JSON value (map, entities, assets, scripts)"},
    "import": {"world": "a value from export"},
    "set_map": {"rows": "replace the whole map with these rows"},
    "point": {"x": 0, "y": 0, "id": "or an entity", "label": "opt"},
    "editor_state": {"_": "full data the editor renders from"}
  },
  "script_api": {
    "entities": "get(id) set(id,key,val) create(kind,x,y[,props]) create_prefab(name,x,y) destroy(id) find(kind) entities() at(x,y) near(x,y,r) count(kind)",
    "movement": "move_by(id,dx,dy) move_toward(id,x,y) [BFS pathing] path_len(x1,y1,x2,y2)",
    "map": "tile(x,y) set_tile(x,y,ch) walkable(x,y) solid(x,y) width() height()",
    "input": "key(name) = held, pressed(name) = pressed this tick. Names: up down left right space enter a-z 0-9 shift",
    "hud": "hud(key, text) shows text over the game; hud_clear()",
    "tags": "entities may have a 'tags' array prop; tagged(tag) returns entities with that tag",
    "world": "now() rand(n) emit(kind[,data]) state(name[,default]) set_state(name,val) print(x)",
    "hooks": "entity scripts define fn tick(me); a script named 'rules' may define fn rules(), run after entities each tick",
    "look": "entities draw as their sprite (prop 'sprite'), else a colored square with 'glyph' and 'color'"
  }
}"#;

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
        "set_map" => "replace map".into(),
        "redo" => "redo".into(),
        "say" => format!("💬 {}", s("text")),
        "point" => format!("📍 {}", if has("label") { s("label") } else { "look here" }),
        "input" if source != "you" => format!("input {}", clip(&c.to_string())),
        "ui" if source != "you" => format!("ui {}", clip(&c.to_string())),
        _ => return None,
    })
}

/// Commands that change the world, and so get an undo entry first.
fn mutates(c: &Value, cmd: &str) -> bool {
    let has = |k: &str| c.get(k).is_some();
    match cmd {
        "new" | "load" | "exec" | "restore" | "create" | "destroy" | "set" | "paint" | "step" | "resize" | "import" | "set_map" => true,
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
        self.sim.replace_world(w);
        self.path = Some(p);
        // The agent's first look reports changes relative to the world as loaded.
        self.last_look = Some(self.sim.world.borrow().clone());
        Ok(self.state())
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
            "notices": self.notices,
            "requests": self.requests,
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
        let world_diff = self.last_look.as_ref().map(|old| {
            let d = world::diff(old, &self.sim.world.borrow());
            // Only keep the parts that say something.
            let mut m = serde_json::Map::new();
            for (k, v) in d.as_object().unwrap() {
                let empty = v.as_array().is_some_and(|a| a.is_empty()) || v.as_object().is_some_and(|o| o.is_empty());
                if !empty {
                    m.insert(k.clone(), v.clone());
                }
            }
            Value::Object(m)
        });
        let changes = json!({ "by_user_since_last_look": user_changes, "world_diff_since_last_look": world_diff });
        let open_requests: Vec<&Value> = self.requests.iter().filter(|r| r["status"] == "open").collect();
        let mut out = self.look_view();
        out["changes"] = changes;
        out["open_requests"] = json!(open_requests);
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
            .and_then(|id| w.entities.get(&id).map(|e| (e.x, e.y)))
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
                .filter(|(_, e)| e.x == x && e.y == y)
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
            "recent_activity": self.activity.iter().rev().take(12).collect::<Vec<_>>(),
            "undo": self.undo.len(),
            "redo": self.redo.len(),
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
            "help" => Ok(serde_json::from_str(HELP).unwrap()),
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
                    Some(e) => (e["x"].as_i64().unwrap_or(0), e["y"].as_i64().unwrap_or(0)),
                    None => (arg_int(c, "x").unwrap_or(0), arg_int(c, "y").unwrap_or(0)),
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
                let w = self.sim.world.borrow();
                let mut v = serde_json::to_value(&*w).map_err(|e| e.to_string())?;
                v["scripts"] = json!(w.scripts);
                v["format"] = json!("forge-world-1");
                Ok(json!({ "world": v }))
            }
            "import" => {
                let data = c.get("world").ok_or("missing 'world' (an exported forge world)")?;
                let mut w: World = serde_json::from_value(data.clone()).map_err(|e| format!("not a forge world: {e}"))?;
                if let Some(scripts) = data.get("scripts").and_then(Value::as_object) {
                    w.scripts = scripts.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
                }
                let max_id = w.entities.keys().next_back().copied().unwrap_or(0);
                w.next_id = w.next_id.max(max_id + 1);
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
                        (e.x, e.y)
                    }
                    None => (arg_int(c, "x").ok_or("need x,y or id")?, arg_int(c, "y").ok_or("need x,y or id")?),
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
                let (ran, hit) = self.sim.step(ticks, arg_str(c, "until"))?;
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                let evs = self.sim.events_since(start);
                Ok(json!({
                    "tick": self.sim.world.borrow().tick,
                    "ran": ran,
                    "until_hit": hit,
                    "ms": (ms * 10.0).round() / 10.0,
                    "hud": self.sim.world.borrow().hud,
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
                let (from, to) = if cmd == "undo" { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
                let (world, label) = from.pop().ok_or(format!("nothing to {cmd}"))?;
                let current = self.sim.world.borrow().clone();
                to.push((current, label.clone()));
                self.sim.replace_world(world);
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
                    "x" => e.x = v.as_i64().ok_or("x must be an integer")?,
                    "y" => e.y = v.as_i64().ok_or("y must be an integer")?,
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
                let (x, y) = (arg_int(c, "x").ok_or("missing 'x'")?, arg_int(c, "y").ok_or("missing 'y'")?);
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
                    Some((a.first()?.as_i64()?, a.get(1)?.as_i64()?, a.get(2)?.as_i64()?))
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
                let size = [pixels.iter().map(|r| r.chars().count()).max().unwrap_or(0), pixels.len()];
                w.sprites.insert(name.into(), Sprite { palette, pixels });
                Ok(json!({ "name": name, "size": size }))
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
                self.sim.replace_world(World { map, rng: seed, next_id: 1, ..World::default() });
                self.path = arg_str(c, "path").map(PathBuf::from);
                self.ui.selected = None;
                self.ui.rev += 1;
                Ok(self.state())
            }
            "save" => {
                let p = arg_str(c, "path").map(PathBuf::from).or(self.path.clone()).ok_or("no path: give 'path'")?;
                self.sim.world.borrow().save_dir(&p)?;
                self.path = Some(p.clone());
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
                let mut r = self.sim.trials(
                    need_str(c, "from")?,
                    arg_int(c, "runs").unwrap_or(20).max(1) as u64,
                    arg_int(c, "ticks").unwrap_or(500).max(0) as u64,
                    arg_str(c, "until"),
                    need_str(c, "metric")?,
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
            _ => Err(format!("unknown cmd '{cmd}' (try {{\"cmd\":\"help\"}})")),
        }
    }

    /// Runs one command and wraps the result as `{"ok":..,"cmd":..,...}`. Changes get an undo
    /// entry and are logged to the editor's activity feed, tagged with `source`.
    pub fn run(&mut self, c: &Value, source: &str) -> Value {
        let name = c.get("cmd").cloned().unwrap_or(Value::Null);
        let cmd = name.as_str().unwrap_or("?").to_string();
        let note = activity_note(c, &cmd, source).map(|d| format!("{source} · {d}"));
        let undoable = mutates(c, &cmd);
        if undoable {
            let snap = self.sim.world.borrow().clone();
            self.undo.push((snap, format!("{source}: {}", note.as_deref().unwrap_or(&cmd))));
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
            Ok(_) if undoable => self.redo.clear(),
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
}

