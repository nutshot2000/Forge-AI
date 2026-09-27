//! The command catalogue: every command, its arguments and their types. It is the single
//! source for validation (clear errors with suggestions), `help`, and the MCP tool schema.

use serde_json::{json, Map, Value};

/// Argument types. A trailing `?` means null is allowed too (e.g. `"int?"`).
pub struct Arg {
    pub name: &'static str,
    pub ty: &'static str,
    pub required: bool,
    pub desc: &'static str,
}

pub struct Cmd {
    pub name: &'static str,
    pub group: &'static str,
    pub desc: &'static str,
    pub args: &'static [Arg],
}

const fn req(name: &'static str, ty: &'static str, desc: &'static str) -> Arg {
    Arg { name, ty, required: true, desc }
}
const fn opt(name: &'static str, ty: &'static str, desc: &'static str) -> Arg {
    Arg { name, ty, required: false, desc }
}

pub const COMMANDS: &[Cmd] = &[
    // --- orientation ---
    Cmd { name: "look", group: "see", desc: "Everything at once: map view, the user's selection/hover/selected tile, what they changed since your last look, open requests, assets, HUD, recent activity, unsaved/recovery state.", args: &[] },
    Cmd { name: "help", group: "see", desc: "This reference.", args: &[] },
    Cmd { name: "state", group: "see", desc: "Short summary: tick, size, counts by kind, scripts, vars, snapshots.", args: &[] },
    Cmd { name: "get", group: "see", desc: "One entity with all its properties.", args: &[req("id", "int", "entity id")] },
    Cmd { name: "query", group: "see", desc: "Find entities.", args: &[
        opt("kind", "string", "only this kind"), opt("near", "array", "[x, y, radius]"),
        opt("where", "string", "Rhai expression over e, e.g. e.hp < 5"), opt("limit", "int", "max results, default 50")] },
    Cmd { name: "view", group: "see", desc: "ASCII render of the map (or a window of it) with entity glyphs.", args: &[
        opt("x", "int", "left"), opt("y", "int", "top"), opt("w", "int", "width"), opt("h", "int", "height")] },
    Cmd { name: "events", group: "see", desc: "Read the event log.", args: &[
        opt("since", "int", "event seq to start after"), opt("kinds", "array", "only these event kinds"), opt("max_events", "int", "default 50")] },
    Cmd { name: "diff", group: "see", desc: "What changed between a snapshot and now (or another snapshot).", args: &[
        req("from", "string", "snapshot name"), opt("to", "string", "snapshot name; default the current world")] },
    Cmd { name: "editor_state", group: "see", desc: "The full data the editor renders from.", args: &[] },

    // --- entities ---
    Cmd { name: "create", group: "edit", desc: "Add an entity, from a kind or a prefab.", args: &[
        opt("kind", "string", "entity kind (required unless prefab is given)"), opt("prefab", "string", "prefab name"),
        req("x", "number", "left edge in tiles (whole numbers for grid games)"), req("y", "number", "top edge in tiles"),
        opt("props", "object", "properties, may include script/sprite/tags/physics/w/h")] },
    Cmd { name: "set", group: "edit", desc: "Change one entity property.", args: &[
        req("id", "int", "entity id"), req("key", "string", "x, y, kind, script or any prop (sprite, color, glyph, hp, tags...)"),
        opt("value", "any", "any JSON; null removes the prop")] },
    Cmd { name: "destroy", group: "edit", desc: "Remove an entity.", args: &[req("id", "int", "entity id")] },

    // --- map & assets ---
    Cmd { name: "paint", group: "edit", desc: "Set map tiles.", args: &[
        req("tile", "string", "one map character"), opt("cells", "array", "[[x,y], ...]"), opt("rect", "array", "or [x, y, w, h]")] },
    Cmd { name: "resize", group: "edit", desc: "Change the map size, keeping what fits.", args: &[
        req("width", "int", "new width"), req("height", "int", "new height"), opt("fill", "string", "char for new cells, default '.'")] },
    Cmd { name: "set_map", group: "edit", desc: "Replace the whole map.", args: &[req("rows", "array", "array of strings")] },
    Cmd { name: "tile", group: "edit", desc: "Define what a map character means. Omit char to list tile types.", args: &[
        opt("char", "string", "map character"), opt("name", "string?", "display name"), opt("solid", "bool", "blocks movement"),
        opt("platform", "bool", "one-way: bodies land on it from above and pass through from below"), opt("ladder", "bool", "climbable (bodies get on_ladder)"),
        opt("color", "string?", "css color"), opt("sprite", "string?", "sprite name"), opt("delete", "bool", "remove this tile type")] },
    Cmd { name: "sprite", group: "edit", desc: "Pixel art as text. Omit name to list; omit pixels to read one.", args: &[
        opt("name", "string", "sprite name"), opt("pixels", "array", "rows of palette chars, '.' = transparent (e.g. 8x8 or 16x16)"),
        opt("palette", "object", "{char: css color}"), opt("frames", "array", "extra animation frames (each an array of rows, same palette)"),
        opt("fps", "number", "animation speed, default 8"), opt("delete", "bool", "remove the sprite")] },
    Cmd { name: "prefab", group: "edit", desc: "Reusable entity templates. Omit name to list; omit props to read one.", args: &[
        opt("name", "string", "prefab name"), opt("props", "object", "template: kind, script, sprite, hp, tags..."), opt("delete", "bool", "remove the prefab")] },

    // --- behaviour ---
    Cmd { name: "script", group: "code", desc: "Read, list or replace a Rhai script (compile-checked before it is installed).", args: &[
        opt("name", "string", "script name; omit to list"), opt("code", "string", "new source; omit to read")] },
    Cmd { name: "exec", group: "code", desc: "Run Rhai against the live world; variables persist between execs.", args: &[req("code", "string", "Rhai code")] },

    // --- time & testing ---
    Cmd { name: "step", group: "run", desc: "Advance the simulation headlessly (fast).", args: &[
        opt("ticks", "int", "default 1"), opt("until", "string", "Rhai expression; stop when true"),
        opt("kinds", "array", "event kinds to show"), opt("max_events", "int", "default 20"),
        opt("inputs", "object", "a scripted player: keys held from each tick offset, e.g. {\"0\":[\"right\"],\"20\":[\"right\",\"space\"],\"40\":[]}")] },
    Cmd { name: "play", group: "run", desc: "Run in real time so the user can watch and play.", args: &[opt("tps", "number", "ticks per second, default 8")] },
    Cmd { name: "pause", group: "run", desc: "Stop real-time play.", args: &[] },
    Cmd { name: "input", group: "run", desc: "Press keys as a player would (names: up down left right space enter a-z 0-9 shift).", args: &[
        opt("tap", "any", "key or keys pressed for one tick"), opt("down", "any", "key(s) to hold"),
        opt("up", "any", "key(s) to release"), opt("clear", "bool", "release everything")] },
    Cmd { name: "snapshot", group: "run", desc: "Save the full world state in memory.", args: &[req("name", "string", "snapshot name")] },
    Cmd { name: "restore", group: "run", desc: "Rewind to a snapshot.", args: &[req("name", "string", "snapshot name")] },
    Cmd { name: "trials", group: "run", desc: "Replay from a snapshot many times with different seeds and score each run.", args: &[
        req("from", "string", "snapshot name"), opt("runs", "int", "default 20"), opt("ticks", "int", "per run, default 500"),
        opt("until", "string", "stop a run early when true"), req("metric", "string", "Rhai expression scored at the end of each run"),
        opt("detail", "bool", "include every run"), opt("inputs", "object", "scripted player keys per tick offset, as in step")] },
    Cmd { name: "physics", group: "edit", desc: "World physics. Presets: platformer (gravity 60, 60 ticks/s), topdown (no gravity, 60 ticks/s), grid (8 ticks/s). Units are tiles and seconds.", args: &[
        opt("preset", "string", "platformer | topdown | grid"), opt("gravity", "number", "tiles/s² downward"),
        opt("max_fall", "number", "terminal fall speed, tiles/s"), opt("tick_rate", "number", "simulation steps per second")] },
    Cmd { name: "camera", group: "edit", desc: "What the player sees: follow an entity through a window of the map.", args: &[
        opt("follow", "int?", "entity to follow, or null"), opt("view", "array", "[width, height] in tiles; [0,0] = whole map"),
        opt("x", "number", "center x"), opt("y", "number", "center y"), opt("zoom", "number", "1 = normal"),
        opt("lerp", "number", "0..1 follow smoothing per tick (1 = instant)"), opt("bounds", "bool", "keep the view inside the map"),
        opt("shake", "number", "screen shake strength in tiles"), opt("tile_size", "int", "pixels per tile at 1x (default 16)")] },
    Cmd { name: "background", group: "edit", desc: "What's behind the map: a sky gradient and parallax layers (sprites repeated sideways, far to near). With a background, empty tiles are see-through.", args: &[
        opt("sky", "any", "a css color, or [top, bottom] for a gradient; null removes it"),
        opt("layers", "array", "[{sprite, parallax (0 = fixed, 1 = moves with the map), y (top, in tiles), height (tiles), repeat (default true)}]"),
        opt("clear", "bool", "remove the whole background first")] },
    Cmd { name: "sound", group: "edit", desc: "A sound effect as a recipe (synthesized in the browser). Omit name to list sounds and presets. Scripts play it with sfx(name).", args: &[
        opt("name", "string", "sound name"), opt("preset", "string", "start from: jump coin hit explosion powerup laser blip stomp win"),
        opt("wave", "string", "square | sine | triangle | saw | noise"), opt("freq", "number", "starting pitch, Hz"),
        opt("slide", "number", "pitch change per second, Hz (negative falls)"), opt("dur", "number", "seconds"), opt("vol", "number", "0..1"),
        opt("attack", "number", "fade-in seconds"), opt("vibrato", "number", "depth, Hz"), opt("vibrato_rate", "number", "speed, Hz"),
        opt("arp", "array", "semitone steps, e.g. [0, 4, 7]"), opt("arp_speed", "number", "seconds per step"), opt("delete", "bool", "remove it")] },
    Cmd { name: "goto", group: "files", desc: "Switch to another level (a world folder next to this one), like goto_level() in a script. Game-wide values (game()/set_game()) carry over.", args: &[
        req("level", "string", "level (world folder) name"), opt("keep_game", "bool", "carry game-wide values over (default true)")] },
    Cmd { name: "behaviors", group: "code", desc: "The behaviour library: ready-made scripts (platformer_player, topdown_player, patrol, chaser, shooter, projectile, health, pickup, door, key, button, moving_platform, goal, spawner) with what they do and the props they read.", args: &[] },
    Cmd { name: "use_behavior", group: "code", desc: "Install a behaviour (and what it requires) into this world as a script, optionally attaching it to entities (their behaviors list; several run together).", args: &[
        req("name", "string", "behaviour name"), opt("on", "any", "entity id or [ids] to attach it to"),
        opt("overwrite", "bool", "replace a script of the same name you've edited (default: keep yours)")] },
    Cmd { name: "export_game", group: "files", desc: "Export the game as one self-contained .html file anyone can play in a browser.", args: &[
        opt("title", "string", "game title (default: the world's name)")] },
    Cmd { name: "undo", group: "run", desc: "Undo the last change (the user's or yours).", args: &[] },
    Cmd { name: "redo", group: "run", desc: "Redo.", args: &[] },
    Cmd { name: "batch", group: "run", desc: "Run several commands as one step: one undo entry; atomic = all or nothing; preview = report the changes without keeping them.", args: &[
        req("commands", "array", "command objects"), opt("atomic", "bool", "undo everything if any command fails"),
        opt("preview", "bool", "run, report the diff, then roll back")] },

    // --- working with the user ---
    Cmd { name: "ui", group: "user", desc: "Drive the user's editor: select things, pick tools, open scripts.", args: &[
        opt("selected", "int?", "entity id, or null"), opt("cell", "array?", "[x, y] map tile to select, or null"),
        opt("tool", "string", "select | paint | place | erase"), opt("tile", "string", "paint character"),
        opt("place", "string", "what the place tool puts down, e.g. prefab:coin"), opt("tab", "string", "activity | scripts | requests | assets | console"),
        opt("script", "string?", "script to open"), opt("hover", "array?", "[x, y] (the editor reports this)")] },
    Cmd { name: "say", group: "user", desc: "Show the user a message in the editor.", args: &[req("text", "string", "message")] },
    Cmd { name: "point", group: "user", desc: "Highlight a spot on the user's map.", args: &[
        opt("x", "number", "tile x"), opt("y", "number", "tile y"), opt("id", "int", "or an entity"), opt("label", "string", "short label")] },
    Cmd { name: "request", group: "user", desc: "Pin a request on an entity or tile (the user does this by right-clicking).", args: &[
        req("text", "string", "what should happen"), opt("id", "int", "entity it's about"), opt("x", "number", "or a tile"), opt("y", "number", "")] },
    Cmd { name: "requests", group: "user", desc: "List requests.", args: &[opt("all", "bool", "include resolved ones")] },
    Cmd { name: "resolve", group: "user", desc: "Answer a request; the user sees your reply.", args: &[
        req("rid", "int", "request id"), opt("reply", "string", "what you did"), opt("status", "string", "done (default) or declined")] },

    Cmd { name: "propose", group: "user", desc: "Suggest a change instead of making it: the user sees a card with what it would change and clicks Apply or Dismiss.", args: &[
        req("title", "string", "short description, e.g. 'Make slimes faster'"), req("commands", "array", "the commands to run if applied")] },
    Cmd { name: "proposals", group: "user", desc: "List suggestions.", args: &[opt("all", "bool", "include applied/dismissed ones")] },
    Cmd { name: "apply_proposal", group: "user", desc: "Apply a suggestion (all or nothing).", args: &[req("pid", "int", "proposal id")] },
    Cmd { name: "dismiss_proposal", group: "user", desc: "Dismiss a suggestion.", args: &[req("pid", "int", "proposal id")] },

    // --- files ---
    Cmd { name: "new", group: "files", desc: "Start a new world.", args: &[
        opt("map", "array", "tile rows"), opt("width", "int", "or a walled empty room, default 20"), opt("height", "int", "default 10"),
        opt("seed", "int", "rng seed"), opt("path", "string", "folder for later saves")] },
    Cmd { name: "load", group: "files", desc: "Open a world folder.", args: &[req("path", "string", "folder with world.json and scripts/")] },
    Cmd { name: "save", group: "files", desc: "Save the world to its folder.", args: &[opt("path", "string", "defaults to the loaded folder")] },
    Cmd { name: "export", group: "files", desc: "The whole world as one JSON value (map, entities, assets, scripts).", args: &[] },
    Cmd { name: "import", group: "files", desc: "Replace the world with an exported one.", args: &[req("world", "object", "a value from export")] },
    Cmd { name: "recover", group: "files", desc: "Load unsaved work found from an earlier session (see recovery in look).", args: &[] },
    Cmd { name: "discard_recovery", group: "files", desc: "Throw that unsaved work away.", args: &[] },
    Cmd { name: "worlds", group: "files", desc: "List the worlds in the project (name, size, contents, map preview, which one is open).", args: &[] },
    Cmd { name: "create_world", group: "files", desc: "Create a new world folder and open it.", args: &[
        req("name", "string", "world name (letters, numbers, spaces, - and _)"), opt("width", "int", "default 24"), opt("height", "int", "default 14"),
        opt("copy_of", "string", "start as a copy of this world instead of an empty room")] },
    Cmd { name: "duplicate_world", group: "files", desc: "Copy a world under a new name.", args: &[req("name", "string", "world to copy"), req("to", "string", "new name")] },
    Cmd { name: "rename_world", group: "files", desc: "Rename a world.", args: &[req("name", "string", "current name"), req("to", "string", "new name")] },
    Cmd { name: "delete_world", group: "files", desc: "Move a world to worlds/.trash (recoverable by hand). The open world can't be deleted.", args: &[req("name", "string", "world name")] },
    Cmd { name: "quit", group: "files", desc: "Stop an engine started by the Forge app.", args: &[] },
];

pub fn find(name: &str) -> Option<&'static Cmd> {
    COMMANDS.iter().find(|c| c.name == name)
}

fn type_ok(ty: &str, v: &Value) -> bool {
    let (base, nullable) = match ty.strip_suffix('?') {
        Some(b) => (b, true),
        None => (ty, false),
    };
    if v.is_null() {
        return nullable || base == "any";
    }
    match base {
        "int" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "string" => v.is_string(),
        "bool" => v.is_boolean(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        _ => true,
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_i64() || n.is_u64() => "int",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}

fn closest<'a>(word: &str, options: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    options
        .map(|o| (distance(word, o), o))
        .filter(|(d, o)| *d <= 2.max(o.len() / 3) || o.starts_with(word) || word.starts_with(o))
        .min_by_key(|(d, _)| *d)
        .map(|(_, o)| o)
}

/// Checks a command against the catalogue. Errors explain what to do instead.
pub fn validate(c: &Value) -> Result<&'static Cmd, String> {
    let obj = c.as_object().ok_or("a command must be a JSON object with a 'cmd' field")?;
    let name = obj.get("cmd").and_then(Value::as_str).ok_or("missing 'cmd' (try {\"cmd\":\"help\"})")?;
    let Some(spec) = find(name) else {
        let hint = closest(name, COMMANDS.iter().map(|c| c.name)).map(|s| format!(" Did you mean '{s}'?")).unwrap_or_default();
        return Err(format!("unknown cmd '{name}'.{hint} (see {{\"cmd\":\"help\"}})"));
    };
    for (k, v) in obj {
        if k == "cmd" {
            continue;
        }
        let Some(arg) = spec.args.iter().find(|a| a.name == k) else {
            let valid: Vec<&str> = spec.args.iter().map(|a| a.name).collect();
            let hint = closest(k, valid.iter().copied()).map(|s| format!(" Did you mean '{s}'?")).unwrap_or_default();
            let list = if valid.is_empty() { "it takes no arguments".to_string() } else { format!("valid: {}", valid.join(", ")) };
            return Err(format!("{name}: unknown argument '{k}'.{hint} ({list})"));
        };
        if !type_ok(arg.ty, v) {
            return Err(format!("{name}: '{k}' must be {} ({}), got {}", arg.ty.trim_end_matches('?'), arg.desc, kind_of(v)));
        }
    }
    for arg in spec.args.iter().filter(|a| a.required) {
        if !obj.contains_key(arg.name) {
            return Err(format!("{name}: missing '{}' ({}: {})", arg.name, arg.ty, arg.desc));
        }
    }
    Ok(spec)
}

pub const SCRIPT_API: &str = r#"{
  "entities": "get(id) set(id,key,val) create(kind,x,y[,props]) create_prefab(name,x,y) destroy(id) find(kind) entities() at(x,y) near(x,y,r) count(kind) tagged(tag)",
  "movement": "grid: move_by(id,dx,dy) move_toward(id,x,y) [BFS pathing] path_len(x1,y1,x2,y2) path(x1,y1,x2,y2) -> [[x,y],...]; free movement: path_dir(id,x,y) -> [dx,dy] unit direction along the path (multiply by speed for set_vel)",
  "physics": "entity props: physics:true (dynamic body) or \"kinematic\"; vx vy (tiles/s); w h (size, default 1); gravity (multiplier); drag; bounce; max_speed; collide:false; solid:true or \"platform\" (blocks bodies, carries riders). Engine writes on_ground, hit_wall (-1/0/1), hit_ceiling, on_ladder, ground_id. Functions: set_vel(id,vx,vy) push(id,ax,ay) jump(id,height_in_tiles) on_ground(id) center(id) dist(a,b) overlaps(a,b) touching(id) overlaps_tile(id,ch) raycast(x1,y1,x2,y2) can_see(a,b) dt() gravity() approach(cur,target,step) clamp(v,lo,hi) sign(v)",
  "camera": "camera_follow(id) camera_shake(tiles) camera_zoom(z)",
  "tiles": "tile types can be solid, platform (one-way, land from above) or ladder",
  "map": "tile(x,y) set_tile(x,y,ch) walkable(x,y) solid(x,y) width() height()",
  "input": "key(name) = held, pressed(name) = pressed this tick. Names: up down left right space enter a-z 0-9 shift",
  "hud": "hud(key, text) shows text over the game; hud_clear()",
  "sound": "sfx(name) plays a sound defined with the sound command",
  "animation": "sprites may have frames + fps (they loop). An entity prop anims: {idle, run, jump, fall, climb} picks the sprite from its movement automatically",
  "world": "now() rand(n) rand_float() emit(kind[,data]) state(name[,default]) set_state(name,val) print(x) prop(id,key,default)",
  "timers": "after(me,ticks,name[,data]) once, every(me,ticks,name[,data]) repeating -> calls fn on_timer(me, name, data) in the entity's script; me = -1 for world timers -> fn on_timer(name, data) in rules. cancel_timer(me,name)",
  "tweens": "tween(id, key, to, ticks[, ease]) smoothly changes x, y, alpha, scale, angle or any number prop. ease: linear | in | out | inout (default) | bounce",
  "messages": "send(id, msg[, data]) and broadcast(msg[, data]) -> fn on_message(me, msg, data) in the receiver's script, delivered the same tick",
  "levels": "goto_level(name) switches to another world folder at the end of the tick (level() = current name). game(name[,default]) / set_game(name,val) are game-wide values that carry across levels (score, lives, keys)",
  "screens": "show_screen(title[, text[, prompt]]) shows a full-screen overlay (title/pause/game over), hide_screen(), screen_shown()",
  "behaviors": "an entity runs its script plus every script named in its behaviors prop (e.g. [\"platformer_player\", \"health\"]); the behaviour library (behaviors / use_behavior commands) has ready-made ones. has_tag(id, tag) has_prefab(name) exists(id)",
  "hooks": "entity scripts define fn tick(me) and optionally fn on_touch(me, other) (called once when two entities start overlapping; other is a map). A script named 'rules' may define fn rules(). Order each tick: tick scripts -> physics -> on_touch -> rules -> camera",
  "units": "positions are the top-left of the entity box, in tiles (1.0 = one tile); grid games use whole numbers",
  "look": "entities draw as their sprite (prop 'sprite'), else a colored square with 'glyph' and 'color'; 'tags' is an array prop"
}"#;

/// The `help` response, generated from the catalogue.
pub fn help() -> Value {
    let mut commands = Map::new();
    for c in COMMANDS {
        let args: Map<String, Value> = c
            .args
            .iter()
            .map(|a| (a.name.to_string(), json!(format!("{}{}: {}", a.ty, if a.required { ", required" } else { "" }, a.desc))))
            .collect();
        commands.insert(c.name.to_string(), json!({ "group": c.group, "does": c.desc, "args": args }));
    }
    json!({
        "protocol": "one JSON object per line in, one per line out. Every command has a 'cmd' field. Unknown commands/arguments and wrong types are rejected with a suggestion.",
        "start_here": "Call look first: it returns the map, what the user has selected/hovered, what they changed since your last look, open requests they pinned on things, assets, HUD and recent activity. The user watches and edits the same world live in the editor; use say to talk to them there, point to show them a spot, and resolve requests with a reply when done. Use batch with atomic for multi-step edits and preview to check changes first.",
        "commands": commands,
        "script_api": serde_json::from_str::<Value>(SCRIPT_API).unwrap(),
    })
}

fn json_type(ty: &str) -> Value {
    let (base, nullable) = match ty.strip_suffix('?') {
        Some(b) => (b, true),
        None => (ty, false),
    };
    let t = match base {
        "int" => json!("integer"),
        "number" => json!("number"),
        "string" => json!("string"),
        "bool" => json!("boolean"),
        "array" => json!("array"),
        "object" => json!("object"),
        _ => return json!({}),
    };
    if nullable { json!({ "type": [t, "null"] }) } else { json!({ "type": t }) }
}

/// JSON Schema for one command object (used for the MCP tool's `commands` items).
pub fn command_schema(c: &Cmd) -> Value {
    let mut props = Map::new();
    props.insert("cmd".into(), json!({ "const": c.name, "description": c.desc }));
    for a in c.args {
        let mut s = json_type(a.ty);
        s["description"] = json!(a.desc);
        props.insert(a.name.into(), s);
    }
    let mut required = vec![json!("cmd")];
    required.extend(c.args.iter().filter(|a| a.required).map(|a| json!(a.name)));
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}
