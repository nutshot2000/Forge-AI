//! Level checks: finds problems in a world before a player does. Everything here is a
//! heuristic that errs on the side of flagging; each issue says where it is and why.

use crate::physics::{self, Body};
use crate::world::{Entity, World};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

pub struct Issue {
    pub severity: &'static str, // error | warning | info
    pub kind: &'static str,
    pub message: String,
    pub id: Option<u64>,
    pub at: Option<(i64, i64)>,
}

impl Issue {
    pub fn json(&self) -> Value {
        let mut v = json!({ "severity": self.severity, "kind": self.kind, "message": self.message });
        if let Some(id) = self.id {
            v["id"] = json!(id);
        }
        if let Some((x, y)) = self.at {
            v["at"] = json!([x, y]);
        }
        v
    }
}

fn names(e: &Entity) -> Vec<String> {
    e.scripts()
}

fn is_player(e: &Entity) -> bool {
    e.props.get("tags").and_then(Value::as_array).is_some_and(|t| t.iter().any(|v| v == "player"))
        || names(e).iter().any(|s| s == "platformer_player" || s == "topdown_player")
}

/// Things a player is meant to reach: pickups, keys, goals, coins.
fn is_goal_like(e: &Entity) -> bool {
    let tags: Vec<&str> = e.props.get("tags").and_then(Value::as_array).map(|t| t.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    tags.contains(&"pickup")
        || tags.contains(&"goal")
        || names(e).iter().any(|s| matches!(s.as_str(), "pickup" | "key" | "goal"))
        || matches!(e.kind.as_str(), "coin" | "gem" | "key" | "flag" | "goal" | "exit" | "star")
}

fn solid_at(w: &World, x: i64, y: i64) -> bool {
    y >= 0 && y < w.height() && !w.walkable(x, y)
}

thread_local! {
    /// Cells just above solid entities (and along moving platforms' paths): extra ground.
    static EXTRA_FLOOR: std::cell::RefCell<HashSet<(i64, i64)>> = std::cell::RefCell::new(HashSet::new());
}

/// Solid entities (blocks, moving platforms) are ground too; a moving platform counts along its
/// whole path (x0/start .. + range), since the player can ride it.
fn entity_floor(w: &World) -> HashSet<(i64, i64)> {
    let mut out = HashSet::new();
    for e in w.entities.values().filter(|e| e.flag("solid")) {
        let top = e.y.floor() as i64 - 1;
        let start = e.props.get("x0").or(e.props.get("start")).and_then(Value::as_f64).unwrap_or(e.x);
        let range = if physics::body(e) == Body::Kinematic { e.f("range", 0.0) } else { 0.0 };
        let (x0, x1) = ((start.min(e.x) + 1e-3).floor() as i64, (start.max(e.x) + range + e.w() - 1e-3).floor() as i64);
        for x in x0..=x1 {
            out.insert((x, top));
        }
    }
    out
}

fn standable(w: &World, x: i64, y: i64) -> bool {
    if x < 0 || x >= w.width() || y < 0 || y >= w.height() || solid_at(w, x, y) {
        return false;
    }
    if EXTRA_FLOOR.with(|f| f.borrow().contains(&(x, y))) {
        return true;
    }
    let below = w.tile(x, y + 1);
    y + 1 < w.height() && (w.is_solid_char(below) || w.tile_def(below).is_some_and(|d| d.platform || d.ladder)) || w.tile_def(w.tile(x, y)).is_some_and(|d| d.ladder)
}

/// Cells a platformer player can stand on or pass through, from a start cell.
/// Model: walk along standable cells, fall, climb ladders, and jump up to `jump` tiles high and
/// `reach` tiles across (checked against solid tiles along a simple arc).
fn platformer_reach(w: &World, start: (i64, i64), jump: i64, reach: i64) -> HashSet<(i64, i64)> {
    // `touched`: every cell the player can be in (standing, jumping, falling); `nodes`: places to stand.
    let mut seen = HashSet::new();
    let mut nodes = HashSet::new();
    let mut q = VecDeque::new();
    // Drop the start onto the ground below it.
    let mut s = start;
    while s.1 + 1 < w.height() && !standable(w, s.0, s.1) && !solid_at(w, s.0, s.1 + 1) {
        s.1 += 1;
    }
    q.push_back(s);
    seen.insert(s);
    nodes.insert(s);
    let clear_column = |x: i64, y0: i64, y1: i64| (y1.min(y0)..=y1.max(y0)).all(|y| !solid_at(w, x, y));
    while let Some((x, y)) = q.pop_front() {
        let mut next = vec![];
        // Jumps (including straight up and short hops), then landing.
        for dy in 0..=jump {
            let top = y - dy;
            if top < 0 || !clear_column(x, y, top) {
                break;
            }
            for dx in -reach..=reach {
                let tx = x + dx;
                // The path across at the top of the jump must be clear.
                let step = if dx >= 0 { 1 } else { -1 };
                let mut ok = true;
                let mut cx = x;
                let mut across = vec![];
                while cx != tx {
                    cx += step;
                    if solid_at(w, cx, top) {
                        ok = false;
                        break;
                    }
                    across.push((cx, top));
                }
                if !ok || tx < 0 || tx >= w.width() {
                    continue;
                }
                seen.extend(across);
                // Fall from (tx, top) until standing on something.
                let mut ty = top;
                while ty + 1 < w.height() && !standable(w, tx, ty) && !solid_at(w, tx, ty + 1) {
                    ty += 1;
                }
                if ty < w.height() && !solid_at(w, tx, ty) {
                    next.push((tx, ty));
                    // Everything passed through in the air counts as touched.
                    for yy in top..=ty {
                        seen.insert((tx, yy));
                    }
                    for yy in top..=y {
                        seen.insert((x, yy));
                    }
                }
            }
        }
        // Ladders go up.
        if w.tile_def(w.tile(x, y)).is_some_and(|d| d.ladder) || w.tile_def(w.tile(x, y - 1)).is_some_and(|d| d.ladder) {
            if !solid_at(w, x, y - 1) {
                next.push((x, y - 1));
            }
        }
        for n in next {
            seen.insert(n);
            if standable(w, n.0, n.1) && nodes.insert(n) {
                q.push_back(n);
            }
        }
    }
    seen
}

/// Cells reachable by walking (top-down / grid games).
fn walk_reach(w: &World, start: (i64, i64)) -> HashSet<(i64, i64)> {
    let mut seen = HashSet::from([start]);
    let mut q = VecDeque::from([start]);
    while let Some((x, y)) = q.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, y + dy);
            if w.walkable(n.0, n.1) && seen.insert(n) {
                q.push_back(n);
            }
        }
    }
    seen
}

/// Sounds a script plays with sfx("literal").
fn sfx_literals(src: &str) -> Vec<String> {
    src.split("sfx(").skip(1).filter_map(|p| {
        let p = p.trim_start();
        let q = p.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = p[1..].find(q)?;
        Some(p[1..1 + end].to_string())
    }).collect()
}

pub fn check(w: &World, compile_errors: &BTreeMap<String, String>, level_exists: &dyn Fn(&str) -> bool) -> Vec<Issue> {
    let mut out = vec![];
    let mut issue = |severity, kind, message: String, id: Option<u64>, at: Option<(i64, i64)>| out.push(Issue { severity, kind, message, id, at });

    for (name, err) in compile_errors {
        issue("error", "script_error", format!("script '{name}' doesn't compile: {err}"), None, None);
    }
    let reported: BTreeSet<String> = w.events.iter().filter(|e| e.kind == "script_error").map(|e| format!("{}: {}", e.data["script"].as_str().unwrap_or("?"), e.data["error"].as_str().unwrap_or("?"))).collect();
    for r in reported {
        issue("error", "script_error", format!("a script failed while running: {r}"), None, None);
    }

    // --- references ---
    let (mw, mh) = (w.width(), w.height());
    for (id, e) in &w.entities {
        let at = Some(e.cell());
        for s in names(e) {
            if !w.scripts.contains_key(&s) {
                issue("error", "missing_script", format!("#{id} {} uses script '{s}', which doesn't exist (use_behavior can install library ones)", e.kind), Some(*id), at);
            }
        }
        if let Some(sp) = e.props.get("sprite").and_then(Value::as_str) {
            if !w.sprites.contains_key(sp) {
                issue("warning", "missing_sprite", format!("#{id} {} uses sprite '{sp}', which doesn't exist", e.kind), Some(*id), at);
            }
        }
        for a in e.props.get("anims").and_then(Value::as_object).into_iter().flat_map(|m| m.values()).filter_map(Value::as_str) {
            if !w.sprites.contains_key(a) {
                issue("warning", "missing_sprite", format!("#{id} {} animation uses sprite '{a}', which doesn't exist", e.kind), Some(*id), at);
            }
        }
        if names(e).iter().any(|s| s == "button") {
            match e.props.get("target").and_then(Value::as_u64) {
                Some(t) if w.entities.contains_key(&t) => {}
                Some(t) => issue("error", "broken_link", format!("button #{id} targets #{t}, which doesn't exist"), Some(*id), at),
                None => issue("warning", "broken_link", format!("button #{id} has no target (set its 'target' prop to a door's id)"), Some(*id), at),
            }
        }
        if let Some(next) = e.props.get("next").and_then(Value::as_str).filter(|n| !n.is_empty()) {
            if !level_exists(next) {
                issue("error", "missing_level", format!("#{id} {} leads to level '{next}', which doesn't exist", e.kind), Some(*id), at);
            }
        }
        // --- placement ---
        if e.x + e.w() <= 0.0 || e.y + e.h() <= 0.0 || e.x >= mw as f64 || e.y >= mh as f64 {
            issue("warning", "outside_map", format!("#{id} {} is outside the map at ({:.1}, {:.1})", e.kind, e.x, e.y), Some(*id), None);
        } else if physics::body(e) == Body::Dynamic {
            let (x0, x1) = ((e.x + 1e-3).floor() as i64, (e.x + e.w() - 1e-3).floor() as i64);
            let (y0, y1) = ((e.y + 1e-3).floor() as i64, (e.y + e.h() - 1e-3).floor() as i64);
            if (y0..=y1).any(|y| (x0..=x1).any(|x| solid_at(w, x, y))) {
                issue("error", "stuck_in_wall", format!("#{id} {} overlaps a solid tile, so it will be stuck", e.kind), Some(*id), at);
            }
        }
    }
    for (name, p) in &w.prefabs {
        for s in p.get("script").and_then(Value::as_str).into_iter().chain(p.get("behaviors").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str)) {
            if !w.scripts.contains_key(s) {
                issue("warning", "missing_script", format!("prefab '{name}' uses script '{s}', which doesn't exist"), None, None);
            }
        }
        if let Some(sp) = p.get("sprite").and_then(Value::as_str) {
            if !w.sprites.contains_key(sp) {
                issue("warning", "missing_sprite", format!("prefab '{name}' uses sprite '{sp}', which doesn't exist"), None, None);
            }
        }
    }
    for (ch, d) in &w.tiles {
        if let Some(sp) = &d.sprite {
            if !w.sprites.contains_key(sp) {
                issue("warning", "missing_sprite", format!("tile '{ch}' uses sprite '{sp}', which doesn't exist"), None, None);
            }
        }
    }
    for l in &w.background.layers {
        if !w.sprites.contains_key(&l.sprite) {
            issue("warning", "missing_sprite", format!("background layer uses sprite '{}', which doesn't exist", l.sprite), None, None);
        }
    }
    if let Some(f) = w.camera.follow {
        if !w.entities.contains_key(&f) {
            issue("warning", "broken_link", format!("the camera follows #{f}, which doesn't exist"), None, None);
        }
    }
    for (script, src) in &w.scripts {
        for snd in sfx_literals(src) {
            if !w.sounds.contains_key(&snd) {
                issue("info", "missing_sound", format!("script '{script}' plays sound '{snd}', which isn't defined (it will be silent; add it with the sound command)"), None, None);
            }
        }
    }

    // --- reachability ---
    let players: Vec<(u64, &Entity)> = w.entities.iter().filter(|(_, e)| is_player(e)).map(|(id, e)| (*id, e)).collect();
    let goals: Vec<(u64, &Entity)> = w.entities.iter().filter(|(_, e)| is_goal_like(e) && !is_player(e)).map(|(id, e)| (*id, e)).collect();
    if players.is_empty() {
        if !goals.is_empty() {
            issue("info", "no_player", "nothing is tagged \"player\", so reachability wasn't checked".into(), None, None);
        }
    } else if let Some((pid, p)) = players.first() {
        let platformer = w.physics.gravity > 0.0 && physics::body(p) == Body::Dynamic;
        let reach = if platformer {
            EXTRA_FLOOR.with(|f| *f.borrow_mut() = entity_floor(w));
            let g = w.physics.gravity * p.f("gravity", 1.0).max(0.1);
            let jh = p.f("jump_height", 3.3);
            let speed = p.f("speed", 7.5);
            let airtime = 2.0 * (2.0 * jh / g).sqrt();
            // Horizontal distance in cells: most of the ideal arc, plus the slack a narrow body has.
            let across = speed * airtime * 0.9 + (1.0 - p.w()).max(0.0);
            platformer_reach(w, p.cell(), jh.floor() as i64, across.floor().max(1.0) as i64)
        } else {
            walk_reach(w, p.cell())
        };
        let mut unreachable = 0;
        for (id, e) in &goals {
            let (cx, cy) = e.cell();
            let near = reach.contains(&(cx, cy)) || [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|(dx, dy)| reach.contains(&(cx + dx, cy + dy)) && !solid_at(w, cx, cy));
            if !near {
                unreachable += 1;
                issue("warning", "unreachable", format!("#{id} {} at ({cx},{cy}) looks unreachable for player #{pid}{}", e.kind, if platformer { " (too high or far to jump to, or walled off)" } else { " (walled off)" }), Some(*id), Some((cx, cy)));
            }
        }
        if !goals.is_empty() && unreachable == 0 {
            issue("info", "reachable", format!("all {} pickups/goals look reachable for player #{pid}", goals.len()), None, None);
        }
    }

    // --- tidiness ---
    let mut used_sprites: HashSet<String> = HashSet::new();
    let all_src: String = w.scripts.values().cloned().collect::<Vec<_>>().join("\n");
    for e in w.entities.values() {
        used_sprites.extend(e.props.get("sprite").and_then(Value::as_str).map(String::from));
        used_sprites.extend(e.props.get("anims").and_then(Value::as_object).into_iter().flat_map(|m| m.values()).filter_map(Value::as_str).map(String::from));
    }
    for p in w.prefabs.values() {
        used_sprites.extend(p.get("sprite").and_then(Value::as_str).map(String::from));
        used_sprites.extend(p.get("anims").and_then(Value::as_object).into_iter().flat_map(|m| m.values()).filter_map(Value::as_str).map(String::from));
    }
    used_sprites.extend(w.tiles.values().filter_map(|d| d.sprite.clone()));
    used_sprites.extend(w.background.layers.iter().map(|l| l.sprite.clone()));
    for name in w.sprites.keys() {
        if !used_sprites.contains(name) && !all_src.contains(&format!("\"{name}\"")) {
            issue("info", "unused_sprite", format!("sprite '{name}' isn't used by anything"), None, None);
        }
    }
    let attached: HashSet<String> = w.entities.values().flat_map(names).chain(w.prefabs.values().flat_map(|p| {
        p.get("script").and_then(Value::as_str).map(String::from).into_iter().chain(p.get("behaviors").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from))
    })).collect();
    for name in w.scripts.keys() {
        if name != "rules" && !attached.contains(name) && !all_src.contains(&format!("\"{name}\"")) {
            issue("info", "unused_script", format!("script '{name}' isn't attached to anything"), None, None);
        }
    }
    out
}
