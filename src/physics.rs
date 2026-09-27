//! Built-in physics, in tile units and seconds.
//!
//! - `physics: true` (dynamic body): velocity (`vx`, `vy`), world gravity × `gravity`,
//!   optional `drag` and `bounce`; collides with solid tiles, one-way `platform` tiles and
//!   solid entities. The engine writes `on_ground`, `hit_wall` (-1/0/1), `hit_ceiling`,
//!   `on_ladder` and `ground_id` back onto the entity every tick.
//! - `physics: "kinematic"`: moves by its velocity and ignores collisions (moving
//!   platforms, projectiles). With `solid: true` (or `"platform"`) it carries whatever
//!   stands on it.
//! - Entities with `solid: true` / `"platform"` block dynamic bodies like tiles do.
//! - `lifetime` (ticks) counts down and removes the entity at 0; `die_on_wall: true` removes a
//!   body when it hits a wall, floor or ceiling. Cheap bullets need no script at all.
//! - The map's sides and top act as walls; below the bottom row is open, so bodies can
//!   fall out of the world (scripts check `y > height()`).

use crate::world::{Entity, World};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const EPS: f64 = 1e-6;
/// Max distance moved per collision sub-step, so fast bodies can't tunnel through tiles.
const MAX_STEP: f64 = 0.45;

#[derive(Clone, Copy, PartialEq)]
pub enum Body {
    None,
    Dynamic,
    Kinematic,
}

pub fn body(e: &Entity) -> Body {
    match e.props.get("physics") {
        Some(Value::String(s)) if s == "kinematic" => Body::Kinematic,
        Some(Value::String(s)) if s.is_empty() || s == "none" => Body::None,
        Some(Value::Bool(true)) | Some(Value::String(_)) => Body::Dynamic,
        Some(Value::Number(n)) if n.as_f64() != Some(0.0) => Body::Dynamic,
        _ => Body::None,
    }
}

/// A solid entity box that bodies collide with.
struct Solid {
    id: u64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    oneway: bool,
}

fn solid_entity(id: u64, e: &Entity) -> Option<Solid> {
    let oneway = match e.props.get("solid") {
        Some(Value::String(s)) if s == "platform" => true,
        _ if e.flag("solid") => false,
        _ => return None,
    };
    Some(Solid { id, x: e.x, y: e.y, w: e.w(), h: e.h(), oneway })
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    X,
    Y,
}

/// If moving the box from `(px, py)` to `(nx, ny)` along `axis` hits something, returns the
/// coordinate to stop at (flush against the obstacle) and the entity id if it was an entity.
fn blocking(w: &World, solids: &[Solid], me: u64, (px, py): (f64, f64), (nx, ny): (f64, f64), bw: f64, bh: f64, axis: Axis) -> Option<(f64, Option<u64>)> {
    let forward = if axis == Axis::X { nx > px } else { ny > py };
    let x0 = (nx + EPS).floor() as i64;
    let x1 = (nx + bw - EPS).floor() as i64;
    let y0 = (ny + EPS).floor() as i64;
    let y1 = (ny + bh - EPS).floor() as i64;
    let mut best: Option<(f64, Option<u64>)> = None;
    let mut consider = |limit: f64, who: Option<u64>| {
        let better = match best {
            None => true,
            Some((b, _)) => if forward { limit < b } else { limit > b },
        };
        if better {
            best = Some((limit, who));
        }
    };
    // Below the map is open (so things can fall into pits); the sides and top are walls.
    let bottom = w.height();
    for ty in y0..=y1.min(bottom - 1) {
        for tx in x0..=x1 {
            let c = w.tile(tx, ty);
            let def = w.tile_def(c);
            let solid = w.is_solid_char(c);
            let platform = def.is_some_and(|d| d.platform);
            if !solid && !platform {
                continue;
            }
            let (tx, ty) = (tx as f64, ty as f64);
            match (axis, forward) {
                // Only obstacles ahead of where we were count, so a body that starts
                // embedded in a wall isn't teleported through it.
                (Axis::X, true) if solid && tx >= px + bw - EPS => consider(tx - bw, None),
                (Axis::X, false) if solid && tx + 1.0 <= px + EPS => consider(tx + 1.0, None),
                (Axis::Y, true) if (solid || platform) && ty >= py + bh - EPS => consider(ty - bh, None),
                (Axis::Y, false) if solid && ty + 1.0 <= py + EPS => consider(ty + 1.0, None),
                _ => {}
            }
        }
    }
    for s in solids.iter().filter(|s| s.id != me) {
        let overlaps = nx < s.x + s.w - EPS && s.x < nx + bw - EPS && ny < s.y + s.h - EPS && s.y < ny + bh - EPS;
        if !overlaps {
            continue;
        }
        match (axis, forward) {
            (Axis::X, true) if !s.oneway && s.x >= px + bw - EPS => consider(s.x - bw, Some(s.id)),
            (Axis::X, false) if !s.oneway && s.x + s.w <= px + EPS => consider(s.x + s.w, Some(s.id)),
            (Axis::Y, true) if s.y >= py + bh - EPS => consider(s.y - bh, Some(s.id)),
            (Axis::Y, false) if !s.oneway && s.y + s.h <= py + EPS => consider(s.y + s.h, Some(s.id)),
            _ => {}
        }
    }
    best
}

/// Moves along one axis in sub-steps. Returns the new coordinate and what blocked it, if anything.
fn sweep(w: &World, solids: &[Solid], me: u64, pos: (f64, f64), delta: f64, bw: f64, bh: f64, axis: Axis) -> (f64, Option<Option<u64>>) {
    let (mut x, mut y) = pos;
    if delta == 0.0 {
        return (if axis == Axis::X { x } else { y }, None);
    }
    let steps = (delta.abs() / MAX_STEP).ceil().max(1.0) as usize;
    let step = delta / steps as f64;
    for _ in 0..steps {
        let (nx, ny) = if axis == Axis::X { (x + step, y) } else { (x, y + step) };
        if let Some((limit, who)) = blocking(w, solids, me, (x, y), (nx, ny), bw, bh, axis) {
            return (limit, Some(who));
        }
        (x, y) = (nx, ny);
    }
    (if axis == Axis::X { x } else { y }, None)
}

fn overlaps_ladder(w: &World, x: f64, y: f64, bw: f64, bh: f64) -> bool {
    for ty in (y + EPS).floor() as i64..=(y + bh - EPS).floor() as i64 {
        for tx in (x + EPS).floor() as i64..=(x + bw - EPS).floor() as i64 {
            if w.tile_def(w.tile(tx, ty)).is_some_and(|d| d.ladder) {
                return true;
            }
        }
    }
    false
}

/// Advances every body by one tick.
pub fn step(w: &mut World) {
    let dt = w.dt();
    // Lifetimes (any entity with `lifetime`, not just bodies).
    let expired: Vec<u64> = w
        .entities
        .iter_mut()
        .filter_map(|(id, e)| {
            let life = e.props.get("lifetime").and_then(Value::as_f64)?;
            e.props.insert("lifetime".into(), json!(life - 1.0));
            (life <= 1.0).then_some(*id)
        })
        .collect();
    for id in expired {
        w.entities.remove(&id);
    }

    let bodies: Vec<(u64, Body)> = w.entities.iter().map(|(id, e)| (*id, body(e))).filter(|(_, b)| *b != Body::None).collect();
    if bodies.is_empty() {
        return;
    }

    // 1. Kinematic movers go first and remember how far they moved, so riders can follow.
    let mut moved: HashMap<u64, (f64, f64)> = HashMap::new();
    for (id, b) in &bodies {
        if *b == Body::Kinematic {
            let e = w.entities.get_mut(id).unwrap();
            let (dx, dy) = (e.f("vx", 0.0) * dt, e.f("vy", 0.0) * dt);
            e.x += dx;
            e.y += dy;
            if dx != 0.0 || dy != 0.0 {
                moved.insert(*id, (dx, dy));
            }
        }
    }

    let solids: Vec<Solid> = w.entities.iter().filter_map(|(id, e)| solid_entity(*id, e)).collect();
    let gravity = w.physics.gravity;
    let max_fall = w.physics.max_fall;

    // 2. Dynamic bodies, in id order (deterministic).
    let mut crashed = vec![];
    for (id, b) in &bodies {
        if *b != Body::Dynamic {
            continue;
        }
        let Some(e) = w.entities.get(id) else { continue };
        let (bw, bh) = (e.w(), e.h());
        let (mut x, mut y) = (e.x, e.y);
        let (mut vx, mut vy) = (e.f("vx", 0.0), e.f("vy", 0.0));
        let g = gravity * e.f("gravity", 1.0);
        let drag = e.f("drag", 0.0);
        let bounce = e.f("bounce", 0.0);
        let collide = e.props.get("collide").map_or(true, |v| v != &json!(false));

        // Ride whatever we were standing on.
        if let Some(gid) = e.props.get("ground_id").and_then(Value::as_u64) {
            if let Some((dx, dy)) = moved.get(&gid) {
                x = if collide { sweep(w, &solids, *id, (x, y), *dx, bw, bh, Axis::X).0 } else { x + dx };
                y += dy;
            }
        }

        vy += g * dt;
        if g > 0.0 {
            vy = vy.min(max_fall);
        }
        if drag > 0.0 {
            let k = (1.0 - drag * dt).max(0.0);
            vx *= k;
            if gravity == 0.0 {
                vy *= k;
            }
        }
        if let Some(ms) = e.props.get("max_speed").and_then(Value::as_f64) {
            let s = (vx * vx + vy * vy).sqrt();
            if s > ms && s > 0.0 {
                vx *= ms / s;
                vy *= ms / s;
            }
        }

        let (mut hit_wall, mut on_ground, mut hit_ceiling, mut ground_id) = (0i64, false, false, None);
        if collide {
            let (nx, bx) = sweep(w, &solids, *id, (x, y), vx * dt, bw, bh, Axis::X);
            x = nx;
            if bx.is_some() {
                hit_wall = if vx > 0.0 { 1 } else { -1 };
                vx = -vx * bounce;
            }
            let falling = vy >= 0.0;
            let (ny, by) = sweep(w, &solids, *id, (x, y), vy * dt, bw, bh, Axis::Y);
            y = ny;
            if let Some(who) = by {
                if falling {
                    on_ground = true;
                    ground_id = who;
                } else {
                    hit_ceiling = true;
                }
                vy = if bounce > 0.0 && vy.abs() > 2.0 { -vy * bounce } else { 0.0 };
            } else if falling && g > 0.0 {
                // Resting exactly on something (vy was 0): probe a hair below.
                if let Some((_, who)) = blocking(w, &solids, *id, (x, y), (x, y + 0.01), bw, bh, Axis::Y) {
                    on_ground = true;
                    ground_id = who;
                }
            }
        } else {
            x += vx * dt;
            y += vy * dt;
        }
        let on_ladder = overlaps_ladder(w, x, y, bw, bh);
        if (hit_wall != 0 || hit_ceiling || on_ground) && w.entities[id].flag("die_on_wall") {
            crashed.push(*id);
        }

        let e = w.entities.get_mut(id).unwrap();
        e.x = x;
        e.y = y;
        let p = &mut e.props;
        p.insert("vx".into(), json!(vx));
        p.insert("vy".into(), json!(vy));
        p.insert("on_ground".into(), json!(on_ground));
        p.insert("hit_wall".into(), json!(hit_wall));
        p.insert("hit_ceiling".into(), json!(hit_ceiling));
        p.insert("on_ladder".into(), json!(on_ladder));
        match ground_id {
            Some(g) => p.insert("ground_id".into(), json!(g)),
            None => p.remove("ground_id"),
        };
    }
    for id in crashed {
        if let Some(e) = w.entities.remove(&id) {
            w.emit("crashed", json!({ "id": id, "kind": e.kind, "x": e.x, "y": e.y }));
        }
    }
}

/// All pairs of overlapping entities (a < b), using a coarse grid so it stays fast.
pub fn contacts(w: &World) -> BTreeSet<(u64, u64)> {
    let mut grid: HashMap<(i64, i64), Vec<u64>> = HashMap::new();
    for (id, e) in &w.entities {
        let (x0, x1) = ((e.x + EPS).floor() as i64, (e.x + e.w() - EPS).floor() as i64);
        let (y0, y1) = ((e.y + EPS).floor() as i64, (e.y + e.h() - EPS).floor() as i64);
        if (x1 - x0 + 1) * (y1 - y0 + 1) > 400 {
            continue; // huge trigger areas are handled below, pairwise
        }
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                grid.entry((cx, cy)).or_default().push(*id);
            }
        }
    }
    let mut pairs = BTreeSet::new();
    for ids in grid.values() {
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                let (a, b) = if a < b { (*a, *b) } else { (*b, *a) };
                if !pairs.contains(&(a, b)) && w.entities[&a].overlaps(&w.entities[&b]) {
                    pairs.insert((a, b));
                }
            }
        }
    }
    // Entities too big for the grid: check them against everything.
    let big: Vec<u64> = w.entities.iter().filter(|(_, e)| e.w() * e.h() > 400.0).map(|(id, _)| *id).collect();
    for a in big {
        for (b, eb) in &w.entities {
            if a != *b && w.entities[&a].overlaps(eb) {
                pairs.insert(if a < *b { (a, *b) } else { (*b, a) });
            }
        }
    }
    pairs
}

/// Casts a ray through the tile grid. Returns the first solid tile hit: (x, y, distance).
pub fn raycast(w: &World, x1: f64, y1: f64, x2: f64, y2: f64) -> Option<(i64, i64, f64)> {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < EPS {
        return None;
    }
    let (mut cx, mut cy) = (x1.floor() as i64, y1.floor() as i64);
    let (sx, sy) = (dx.signum() as i64, dy.signum() as i64);
    let tdx = if dx != 0.0 { (1.0 / dx).abs() * len } else { f64::INFINITY };
    let tdy = if dy != 0.0 { (1.0 / dy).abs() * len } else { f64::INFINITY };
    let mut tmx = if dx > 0.0 { (cx as f64 + 1.0 - x1) / dx * len } else if dx < 0.0 { (x1 - cx as f64) / -dx * len } else { f64::INFINITY };
    let mut tmy = if dy > 0.0 { (cy as f64 + 1.0 - y1) / dy * len } else if dy < 0.0 { (y1 - cy as f64) / -dy * len } else { f64::INFINITY };
    let mut t = 0.0;
    for _ in 0..4096 {
        if w.is_solid_char(w.tile(cx, cy)) {
            return Some((cx, cy, t));
        }
        if tmx < tmy {
            t = tmx;
            tmx += tdx;
            cx += sx;
        } else {
            t = tmy;
            tmy += tdy;
            cy += sy;
        }
        if t > len {
            return None;
        }
    }
    None
}

/// Moves the camera toward its target, keeps it inside the map and decays shake.
pub fn update_camera(w: &mut World) {
    let (mw, mh) = (w.width() as f64, w.height() as f64);
    let target = w.camera.follow.and_then(|id| w.entities.get(&id)).map(|e| e.center());
    let cam = &mut w.camera;
    if cam.view[0] <= 0.0 || cam.view[1] <= 0.0 {
        cam.x = mw / 2.0;
        cam.y = mh / 2.0;
    } else {
        if let Some((tx, ty)) = target {
            let k = cam.lerp.clamp(0.0, 1.0);
            let far = (tx - cam.x).abs() > cam.view[0] || (ty - cam.y).abs() > cam.view[1];
            if k >= 1.0 || far || (cam.x == 0.0 && cam.y == 0.0) {
                cam.x = tx;
                cam.y = ty;
            } else {
                cam.x += (tx - cam.x) * k;
                cam.y += (ty - cam.y) * k;
            }
        }
        if cam.bounds {
            let zoom = if cam.zoom > 0.0 { cam.zoom } else { 1.0 };
            let (hw, hh) = (cam.view[0] / zoom / 2.0, cam.view[1] / zoom / 2.0);
            cam.x = if mw <= hw * 2.0 { mw / 2.0 } else { cam.x.clamp(hw, mw - hw) };
            cam.y = if mh <= hh * 2.0 { mh / 2.0 } else { cam.y.clamp(hh, mh - hh) };
        }
    }
    cam.shake = if cam.shake < 0.01 { 0.0 } else { cam.shake * 0.88 };
}

/// Initial jump speed needed to rise `height` tiles under `gravity`.
pub fn jump_speed(gravity: f64, height: f64) -> Option<f64> {
    (gravity > 0.0 && height > 0.0).then(|| (2.0 * gravity * height).sqrt())
}

/// The compact per-tick picture the editor and web player draw.
pub fn frame_entities(w: &World) -> Vec<Value> {
    w.entities
        .iter()
        .map(|(id, e)| {
            let mut extra = BTreeMap::new();
            for k in ["w", "h", "flip", "angle", "alpha", "scale", "z", "vx", "vy", "anims", "on_ground", "on_ladder", "fit", "label", "label_color"] {
                if let Some(v) = e.props.get(k) {
                    if !v.is_null() {
                        extra.insert(k, v.clone());
                    }
                }
            }
            json!([id, e.glyph().to_string(), e.x, e.y, e.kind, e.props.get("color"), e.props.get("sprite"), extra])
        })
        .collect()
}
