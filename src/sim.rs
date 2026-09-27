//! The simulation: a world plus a Rhai engine whose API reads and writes that world.
//!
//! Each tick: entity scripts (`fn tick(me)`) → physics → touch events
//! (`fn on_touch(me, other)`) → the `rules` script (`fn rules()`) → camera.

use crate::physics;
use crate::world::{num, Entity, Event, World};
use rhai::{Array, CallFnOptions, Dynamic, Engine, EvalAltResult, Map as RMap, Scope, AST};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

type W = Rc<RefCell<World>>;
type RR<T> = Result<T, Box<EvalAltResult>>;

/// Receives a frame for every tick (and for activity notes); set by the editor host.
pub type Recorder = Box<dyn Fn(&World, Option<&str>, Vec<Value>, bool)>;

pub fn to_dyn(v: &Value) -> Dynamic {
    rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT)
}

pub fn to_json(d: &Dynamic) -> Value {
    rhai::serde::from_dynamic::<Value>(d).unwrap_or(Value::Null)
}

/// A script number (int or float) as f64.
fn n(d: &Dynamic) -> RR<f64> {
    if let Ok(i) = d.as_int() {
        return Ok(i as f64);
    }
    d.as_float().map_err(|t| format!("expected a number, got {t}").into())
}

/// A script number as a tile coordinate.
fn cell(d: &Dynamic) -> RR<i64> {
    Ok(n(d)?.floor() as i64)
}

fn ents(w: &World, pred: impl Fn(&Entity) -> bool) -> Array {
    w.entities.iter().filter(|(_, e)| pred(e)).map(|(id, e)| to_dyn(&World::entity_json(*id, e))).collect()
}

fn cheb(a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn ent<'a>(w: &'a World, id: i64, what: &str) -> RR<&'a Entity> {
    w.entities.get(&(id as u64)).ok_or_else(|| format!("{what}: no entity {id}").into())
}

/// Registers a closure that captures its own clone of the world handle.
macro_rules! reg {
    ($eng:ident, $w:ident, $name:literal, $f:expr) => {{
        let $w = $w.clone();
        $eng.register_fn($name, $f);
    }};
}

fn build_engine(w: &W) -> Engine {
    let mut e = Engine::new();
    e.set_max_operations(5_000_000);
    e.set_max_call_levels(64);
    {
        let w = w.clone();
        e.on_print(move |s| w.borrow_mut().emit("print", json!(s)));
    }
    {
        let w = w.clone();
        e.on_debug(move |s, _, _| w.borrow_mut().emit("debug", json!(s)));
    }

    // --- entities ---
    reg!(e, w, "get", move |id: i64| -> Dynamic {
        let w = w.borrow();
        w.entities.get(&(id as u64)).map_or(Dynamic::UNIT, |en| to_dyn(&World::entity_json(id as u64, en)))
    });
    reg!(e, w, "set", move |id: i64, key: &str, v: Dynamic| -> RR<()> {
        let mut w = w.borrow_mut();
        let en = w.entities.get_mut(&(id as u64)).ok_or_else(|| format!("set: no entity {id}"))?;
        match key {
            "id" => return Err("set: id is read-only".into()),
            "x" => en.x = n(&v).map_err(|_| format!("set: x must be a number, got {}", v.type_name()))?,
            "y" => en.y = n(&v).map_err(|_| format!("set: y must be a number, got {}", v.type_name()))?,
            "kind" => en.kind = v.into_string().map_err(|t| format!("set: kind must be string, got {t}"))?,
            "script" => en.script = if v.is_unit() { None } else { Some(v.to_string()) },
            _ if v.is_unit() => {
                en.props.remove(key);
            }
            _ => {
                en.props.insert(key.to_string(), to_json(&v));
            }
        }
        Ok(())
    });
    reg!(e, w, "create", move |kind: &str, x: Dynamic, y: Dynamic| -> RR<i64> {
        Ok(w.borrow_mut().spawn(kind, n(&x)?, n(&y)?, BTreeMap::new()) as i64)
    });
    reg!(e, w, "create", move |kind: &str, x: Dynamic, y: Dynamic, props: RMap| -> RR<i64> {
        let props = props.into_iter().map(|(k, v)| (k.to_string(), to_json(&v))).collect();
        Ok(w.borrow_mut().spawn(kind, n(&x)?, n(&y)?, props) as i64)
    });
    reg!(e, w, "create_prefab", move |name: &str, x: Dynamic, y: Dynamic| -> RR<i64> {
        Ok(w.borrow_mut().spawn_prefab(name, n(&x)?, n(&y)?, BTreeMap::new())? as i64)
    });
    reg!(e, w, "create_prefab", move |name: &str, x: Dynamic, y: Dynamic, props: RMap| -> RR<i64> {
        let props = props.into_iter().map(|(k, v)| (k.to_string(), to_json(&v))).collect();
        Ok(w.borrow_mut().spawn_prefab(name, n(&x)?, n(&y)?, props)? as i64)
    });
    reg!(e, w, "destroy", move |id: i64| -> bool { w.borrow_mut().entities.remove(&(id as u64)).is_some() });
    reg!(e, w, "find", move |kind: &str| -> Array { ents(&w.borrow(), |e| e.kind == kind) });
    reg!(e, w, "entities", move || -> Array { ents(&w.borrow(), |_| true) });
    reg!(e, w, "at", move |x: Dynamic, y: Dynamic| -> RR<Array> {
        let c = (cell(&x)?, cell(&y)?);
        Ok(ents(&w.borrow(), |e| e.cell() == c))
    });
    reg!(e, w, "tagged", move |tag: &str| -> Array {
        ents(&w.borrow(), |e| e.props.get("tags").and_then(Value::as_array).is_some_and(|t| t.iter().any(|v| v.as_str() == Some(tag))))
    });
    reg!(e, w, "count", move |kind: &str| -> i64 { w.borrow().entities.values().filter(|e| e.kind == kind).count() as i64 });
    reg!(e, w, "near", move |x: Dynamic, y: Dynamic, r: Dynamic| -> RR<Array> {
        let w = w.borrow();
        let (fx, fy, fr) = (n(&x)?, n(&y)?, n(&r)?);
        let grid = x.is_int() && y.is_int() && r.is_int();
        // Integers: tiles within r steps (grid games). Floats: distance from the point to centers.
        let d = |e: &Entity| -> f64 {
            if grid {
                cheb(e.cell(), (fx as i64, fy as i64)) as f64
            } else {
                let (cx, cy) = e.center();
                ((cx - fx).powi(2) + (cy - fy).powi(2)).sqrt()
            }
        };
        let mut v: Vec<_> = w.entities.iter().map(|(id, e)| (d(e), *id, e)).filter(|(dist, _, _)| *dist <= fr).collect();
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        Ok(v.into_iter().map(|(_, id, e)| to_dyn(&World::entity_json(id, e))).collect())
    });

    // --- grid movement ---
    reg!(e, w, "move_by", move |id: i64, dx: Dynamic, dy: Dynamic| -> RR<bool> {
        let mut w = w.borrow_mut();
        let en = ent(&w, id, "move_by")?;
        let (nx, ny) = (en.x + n(&dx)?, en.y + n(&dy)?);
        let (cx, cy) = ((nx + en.w() / 2.0).floor() as i64, (ny + en.h() / 2.0).floor() as i64);
        if !w.walkable(cx, cy) {
            return Ok(false);
        }
        let en = w.entities.get_mut(&(id as u64)).unwrap();
        (en.x, en.y) = (nx, ny);
        Ok(true)
    });
    reg!(e, w, "move_toward", move |id: i64, tx: Dynamic, ty: Dynamic| -> RR<bool> {
        let mut w = w.borrow_mut();
        let from = ent(&w, id, "move_toward")?.cell();
        let Some((x, y)) = w.next_step(from, (cell(&tx)?, cell(&ty)?)) else { return Ok(false) };
        let en = w.entities.get_mut(&(id as u64)).unwrap();
        en.x += (x - from.0) as f64;
        en.y += (y - from.1) as f64;
        Ok(true)
    });
    reg!(e, w, "path_len", move |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic| -> RR<i64> {
        Ok(w.borrow().path_len((cell(&x1)?, cell(&y1)?), (cell(&x2)?, cell(&y2)?)).map_or(-1, |d| d as i64))
    });

    reg!(e, w, "path", move |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic| -> RR<Dynamic> {
        let w = w.borrow();
        Ok(match w.path((cell(&x1)?, cell(&y1)?), (cell(&x2)?, cell(&y2)?)) {
            Some(p) => Dynamic::from_array(p.into_iter().map(|(x, y)| Dynamic::from_array(vec![Dynamic::from(x), Dynamic::from(y)])).collect()),
            None => Dynamic::UNIT,
        })
    });
    // Free movement along the grid path: a unit direction toward the next tile's center.
    reg!(e, w, "path_dir", move |id: i64, tx: Dynamic, ty: Dynamic| -> RR<Array> {
        let w = w.borrow();
        let en = ent(&w, id, "path_dir")?;
        let (cx, cy) = en.center();
        let target = (cell(&tx)?, cell(&ty)?);
        let goal = match w.path(en.cell(), target) {
            Some(p) if p.is_empty() => (target.0 as f64 + 0.5, target.1 as f64 + 0.5),
            // Aim a little further along the path when the next tile is close, so corners are smooth.
            Some(p) => {
                let (nx, ny) = p[0];
                let near = ((nx as f64 + 0.5 - cx).powi(2) + (ny as f64 + 0.5 - cy).powi(2)).sqrt() < 0.3;
                let (gx, gy) = if near { *p.get(1).unwrap_or(&p[0]) } else { p[0] };
                (gx as f64 + 0.5, gy as f64 + 0.5)
            }
            None => return Ok(vec![Dynamic::from(0.0), Dynamic::from(0.0)]),
        };
        let (dx, dy) = (goal.0 - cx, goal.1 - cy);
        let len = (dx * dx + dy * dy).sqrt();
        Ok(if len < 0.05 { vec![Dynamic::from(0.0), Dynamic::from(0.0)] } else { vec![Dynamic::from(dx / len), Dynamic::from(dy / len)] })
    });

    // --- physics ---
    reg!(e, w, "set_vel", move |id: i64, vx: Dynamic, vy: Dynamic| -> RR<()> {
        let mut w = w.borrow_mut();
        let en = w.entities.get_mut(&(id as u64)).ok_or_else(|| format!("set_vel: no entity {id}"))?;
        en.props.insert("vx".into(), json!(n(&vx)?));
        en.props.insert("vy".into(), json!(n(&vy)?));
        Ok(())
    });
    reg!(e, w, "push", move |id: i64, ax: Dynamic, ay: Dynamic| -> RR<()> {
        let mut w = w.borrow_mut();
        let en = w.entities.get_mut(&(id as u64)).ok_or_else(|| format!("push: no entity {id}"))?;
        let (vx, vy) = (en.f("vx", 0.0) + n(&ax)?, en.f("vy", 0.0) + n(&ay)?);
        en.props.insert("vx".into(), json!(vx));
        en.props.insert("vy".into(), json!(vy));
        Ok(())
    });
    reg!(e, w, "jump", move |id: i64, height: Dynamic| -> RR<bool> {
        let mut w = w.borrow_mut();
        let g = w.physics.gravity;
        let en = w.entities.get_mut(&(id as u64)).ok_or_else(|| format!("jump: no entity {id}"))?;
        match physics::jump_speed(g * en.f("gravity", 1.0), n(&height)?) {
            Some(v) => {
                en.props.insert("vy".into(), json!(-v));
                en.props.insert("on_ground".into(), json!(false));
                Ok(true)
            }
            None => Ok(false),
        }
    });
    reg!(e, w, "on_ground", move |id: i64| -> RR<bool> { Ok(ent(&w.borrow(), id, "on_ground")?.flag("on_ground")) });
    reg!(e, w, "center", move |id: i64| -> RR<Array> {
        let (x, y) = ent(&w.borrow(), id, "center")?.center();
        Ok(vec![Dynamic::from(x), Dynamic::from(y)])
    });
    reg!(e, w, "dist", move |a: i64, b: i64| -> RR<f64> {
        let w = w.borrow();
        let ((ax, ay), (bx, by)) = (ent(&w, a, "dist")?.center(), ent(&w, b, "dist")?.center());
        Ok(((ax - bx).powi(2) + (ay - by).powi(2)).sqrt())
    });
    reg!(e, w, "overlaps", move |a: i64, b: i64| -> RR<bool> {
        let w = w.borrow();
        Ok(ent(&w, a, "overlaps")?.overlaps(ent(&w, b, "overlaps")?))
    });
    reg!(e, w, "touching", move |id: i64| -> RR<Array> {
        let w = w.borrow();
        let me = ent(&w, id, "touching")?;
        Ok(w.entities.iter().filter(|(o, e)| **o != id as u64 && me.overlaps(e)).map(|(o, e)| to_dyn(&World::entity_json(*o, e))).collect())
    });
    reg!(e, w, "overlaps_tile", move |id: i64, ch: &str| -> RR<bool> {
        let w = w.borrow();
        let e = ent(&w, id, "overlaps_tile")?;
        let c = ch.chars().next().unwrap_or('#');
        let (x0, x1) = ((e.x + 1e-6).floor() as i64, (e.x + e.w() - 1e-6).floor() as i64);
        let (y0, y1) = ((e.y + 1e-6).floor() as i64, (e.y + e.h() - 1e-6).floor() as i64);
        Ok((y0..=y1).any(|y| (x0..=x1).any(|x| w.tile(x, y) == c)))
    });
    reg!(e, w, "raycast", move |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic| -> RR<Dynamic> {
        let w = w.borrow();
        Ok(match physics::raycast(&w, n(&x1)?, n(&y1)?, n(&x2)?, n(&y2)?) {
            Some((x, y, d)) => to_dyn(&json!({ "x": x, "y": y, "dist": d, "tile": w.tile(x, y).to_string() })),
            None => Dynamic::UNIT,
        })
    });
    reg!(e, w, "can_see", move |a: i64, b: i64| -> RR<bool> {
        let w = w.borrow();
        let ((ax, ay), (bx, by)) = (ent(&w, a, "can_see")?.center(), ent(&w, b, "can_see")?.center());
        Ok(physics::raycast(&w, ax, ay, bx, by).is_none())
    });
    reg!(e, w, "dt", move || -> f64 { w.borrow().dt() });
    reg!(e, w, "gravity", move || -> f64 { w.borrow().physics.gravity });
    e.register_fn("approach", |cur: Dynamic, target: Dynamic, step: Dynamic| -> RR<f64> {
        let (c, t, s) = (n(&cur)?, n(&target)?, n(&step)?.abs());
        Ok(if c < t { (c + s).min(t) } else { (c - s).max(t) })
    });
    e.register_fn("clamp", |v: Dynamic, lo: Dynamic, hi: Dynamic| -> RR<f64> { Ok(n(&v)?.clamp(n(&lo)?, n(&hi)?)) });
    e.register_fn("sign", |v: Dynamic| -> RR<i64> { Ok(n(&v)?.signum() as i64 * i64::from(n(&v)? != 0.0)) });

    // --- camera ---
    reg!(e, w, "camera_follow", move |id: i64| { w.borrow_mut().camera.follow = (id >= 0).then_some(id as u64) });
    reg!(e, w, "camera_shake", move |amount: Dynamic| -> RR<()> {
        let mut w = w.borrow_mut();
        w.camera.shake = w.camera.shake.max(n(&amount)?);
        Ok(())
    });
    reg!(e, w, "camera_zoom", move |z: Dynamic| -> RR<()> {
        w.borrow_mut().camera.zoom = n(&z)?.clamp(0.1, 10.0);
        Ok(())
    });

    // --- map ---
    reg!(e, w, "tile", move |x: Dynamic, y: Dynamic| -> RR<String> { Ok(w.borrow().tile(cell(&x)?, cell(&y)?).to_string()) });
    reg!(e, w, "set_tile", move |x: Dynamic, y: Dynamic, c: &str| -> RR<bool> {
        let (x, y) = (cell(&x)?, cell(&y)?);
        Ok(c.chars().next().is_some_and(|c| w.borrow_mut().set_tile(x, y, c)))
    });
    reg!(e, w, "walkable", move |x: Dynamic, y: Dynamic| -> RR<bool> { Ok(w.borrow().walkable(cell(&x)?, cell(&y)?)) });
    reg!(e, w, "solid", move |x: Dynamic, y: Dynamic| -> RR<bool> { Ok(!w.borrow().walkable(cell(&x)?, cell(&y)?)) });
    reg!(e, w, "width", move || -> i64 { w.borrow().width() });
    reg!(e, w, "height", move || -> i64 { w.borrow().height() });

    // --- input & HUD ---
    reg!(e, w, "key", move |k: &str| -> bool { w.borrow().input.down.contains(&k.to_lowercase()) });
    reg!(e, w, "pressed", move |k: &str| -> bool { w.borrow().input.pressed.contains(&k.to_lowercase()) });
    reg!(e, w, "hud", move |k: &str, text: Dynamic| {
        let t = if text.is_string() { text.into_string().unwrap_or_default() } else { text.to_string() };
        w.borrow_mut().hud.insert(k.to_string(), t);
    });
    reg!(e, w, "hud_clear", move || { w.borrow_mut().hud.clear() });

    // --- world state, events, randomness ---
    reg!(e, w, "now", move || -> i64 { w.borrow().tick as i64 });
    reg!(e, w, "rand", move |n: i64| -> i64 {
        if n <= 0 { 0 } else { (w.borrow_mut().next_rand() % n as u64) as i64 }
    });
    reg!(e, w, "rand_float", move || -> f64 { (w.borrow_mut().next_rand() >> 11) as f64 / (1u64 << 53) as f64 });
    reg!(e, w, "emit", move |kind: &str| { w.borrow_mut().emit(kind, Value::Null) });
    reg!(e, w, "emit", move |kind: &str, data: Dynamic| { w.borrow_mut().emit(kind, to_json(&data)) });
    reg!(e, w, "state", move |name: &str| -> Dynamic { w.borrow().vars.get(name).map_or(Dynamic::UNIT, to_dyn) });
    reg!(e, w, "state", move |name: &str, default: Dynamic| -> Dynamic { w.borrow().vars.get(name).map_or(default, to_dyn) });
    reg!(e, w, "set_state", move |name: &str, v: Dynamic| {
        let mut w = w.borrow_mut();
        if v.is_unit() {
            w.vars.remove(name);
        } else {
            w.vars.insert(name.to_string(), to_json(&v));
        }
    });
    e
}

/// Held keys per tick offset, for scripted playtests: `{0: ["right"], 30: ["right", "space"], 45: []}`.
pub type InputTimeline = BTreeMap<u64, Vec<String>>;

pub struct Sim {
    pub world: W,
    engine: Engine,
    asts: HashMap<String, AST>,
    pub compile_errors: BTreeMap<String, String>,
    pub snapshots: BTreeMap<String, World>,
    /// Distinct runtime errors already emitted, so a broken script reports once, not every tick.
    reported: HashSet<String>,
    /// Persistent scope for `exec`, so variables survive between commands.
    repl: Scope<'static>,
    /// Where each tick is recorded for the editor; paused during trials and previews.
    pub recorder: Option<Recorder>,
    pub recording: bool,
}

impl Sim {
    pub fn new(world: World) -> Self {
        let world = Rc::new(RefCell::new(world));
        let engine = build_engine(&world);
        let mut sim = Sim {
            world,
            engine,
            asts: HashMap::new(),
            compile_errors: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            reported: HashSet::new(),
            repl: Scope::new(),
            recorder: None,
            recording: true,
        };
        sim.recompile();
        sim
    }

    pub fn replace_world(&mut self, world: World) {
        *self.world.borrow_mut() = world;
        self.recompile();
    }

    pub fn recompile(&mut self) {
        self.asts.clear();
        self.compile_errors.clear();
        self.reported.clear();
        let scripts = self.world.borrow().scripts.clone();
        for (name, src) in scripts {
            match self.engine.compile(&src) {
                Ok(ast) => {
                    self.asts.insert(name, ast);
                }
                Err(e) => {
                    self.compile_errors.insert(name, e.to_string());
                }
            }
        }
    }

    /// Compile-checks and installs a script. Returns the functions it defines.
    pub fn set_script(&mut self, name: &str, code: &str) -> Result<Vec<String>, String> {
        let ast = self.engine.compile(code).map_err(|e| e.to_string())?;
        let fns = ast
            .iter_functions()
            .filter(|f| !f.name.starts_with("anon$"))
            .map(|f| format!("{}({})", f.name, f.params.join(", ")))
            .collect();
        self.world.borrow_mut().scripts.insert(name.into(), code.into());
        self.asts.insert(name.into(), ast);
        self.compile_errors.remove(name);
        self.reported.clear();
        Ok(fns)
    }

    fn report(&mut self, data: Value) {
        let key = format!("{}|{}", data["script"], data["error"]);
        if self.reported.insert(key) {
            self.world.borrow_mut().emit("script_error", data);
        }
    }

    /// Records the current world as an editor frame, with an optional activity note.
    pub fn record(&self, note: Option<&str>, events: Vec<Value>, jump: bool) {
        if let (true, Some(rec)) = (self.recording, &self.recorder) {
            rec(&self.world.borrow(), note, events, jump);
        }
    }

    fn has_fn(&self, script: &str, name: &str, params: usize) -> bool {
        self.asts.get(script).is_some_and(|a| a.iter_functions().any(|f| f.name == name && f.params.len() == params))
    }

    fn call(&mut self, script: &str, func: &str, args: impl rhai::FuncArgs, who: Option<u64>) {
        let opts = CallFnOptions::new().eval_ast(false);
        let res = match self.asts.get(script) {
            Some(ast) => self.engine.call_fn_with_options::<Dynamic>(opts, &mut Scope::new(), ast, func, args).map(|_| ()).map_err(|e| e.to_string()),
            None => Err(format!("script '{script}' is missing or failed to compile")),
        };
        if let Err(msg) = res {
            let mut data = json!({ "script": script, "error": msg });
            if let Some(id) = who {
                data["entity"] = json!(id);
            }
            self.report(data);
        }
    }

    pub fn run_tick(&mut self) {
        let start = self.world.borrow().event_seq;

        // 1. Entity scripts.
        let jobs: Vec<(u64, String)> =
            self.world.borrow().entities.iter().filter_map(|(id, e)| e.script.clone().map(|s| (*id, s))).collect();
        for (id, script) in &jobs {
            // An earlier script this tick may have destroyed it.
            if !self.world.borrow().entities.contains_key(id) {
                continue;
            }
            // Scripts may only react to touches; a missing script is still reported.
            if self.has_fn(script, "tick", 1) || !self.asts.contains_key(script) {
                self.call(script, "tick", (*id as i64,), Some(*id));
            }
        }

        // 2. Physics.
        physics::step(&mut self.world.borrow_mut());

        // 3. Touch events, only if some script listens for them.
        let listeners: BTreeSet<String> = self.asts.keys().filter(|s| self.has_fn(s, "on_touch", 2)).cloned().collect();
        if !listeners.is_empty() {
            let now = physics::contacts(&self.world.borrow());
            let new: Vec<(u64, u64)> = {
                let w = self.world.borrow();
                now.iter().filter(|p| !w.contacts.contains(p)).copied().collect()
            };
            self.world.borrow_mut().contacts = now;
            for (a, b) in new {
                for (me, other) in [(a, b), (b, a)] {
                    let (script, other_map) = {
                        let w = self.world.borrow();
                        let (Some(m), Some(o)) = (w.entities.get(&me), w.entities.get(&other)) else { continue };
                        (m.script.clone(), World::entity_json(other, o))
                    };
                    if let Some(s) = script.filter(|s| listeners.contains(s)) {
                        self.call(&s, "on_touch", (me as i64, to_dyn(&other_map)), Some(me));
                    }
                }
            }
        }

        // 4. Rules.
        if self.has_fn("rules", "rules", 0) {
            self.call("rules", "rules", (), None);
        }

        // 5. Camera, clock, input.
        {
            let mut w = self.world.borrow_mut();
            physics::update_camera(&mut w);
            w.tick += 1;
            w.end_tick_input();
        }
        if self.recording && self.recorder.is_some() {
            let evs = self.events_since(start).into_iter().take(20).map(|e| json!({ "kind": e.kind, "data": e.data })).collect();
            self.record(None, evs, false);
        }
    }

    /// Runs up to `ticks` ticks, stopping early once the `until` expression is true.
    /// `inputs` holds keys down from given tick offsets (a scripted player).
    pub fn step(&mut self, ticks: u64, until: Option<&str>, inputs: Option<&InputTimeline>) -> Result<(u64, bool), String> {
        let until = until.map(|s| self.engine.compile_expression(s).map_err(|e| format!("until: {e}"))).transpose()?;
        for i in 0..ticks {
            if let Some(keys) = inputs.and_then(|t| t.get(&i)) {
                let mut w = self.world.borrow_mut();
                let held: BTreeSet<String> = keys.iter().map(|k| k.to_lowercase()).collect();
                w.input.pressed = held.difference(&w.input.down).cloned().collect();
                w.input.down = held;
            }
            self.run_tick();
            if let Some(ast) = &until {
                let v = self.engine.eval_ast::<Dynamic>(ast).map_err(|e| format!("until: {e}"))?;
                if v.as_bool().unwrap_or(false) {
                    return Ok((i + 1, true));
                }
            }
        }
        if inputs.is_some() {
            self.world.borrow_mut().input = Default::default();
        }
        Ok((ticks, false))
    }

    pub fn exec(&mut self, code: &str) -> Result<Value, String> {
        self.engine.eval_with_scope::<Dynamic>(&mut self.repl, code).map(|d| to_json(&d)).map_err(|e| e.to_string())
    }

    pub fn eval_expr(&self, expr: &str) -> Result<Value, String> {
        self.engine.eval_expression::<Dynamic>(expr).map(|d| to_json(&d)).map_err(|e| e.to_string())
    }

    /// Filters entities by kind, proximity and an arbitrary `where` expression over `e`.
    pub fn query(&self, kind: Option<&str>, near: Option<(f64, f64, f64)>, filter: Option<&str>, limit: usize) -> Result<(usize, Vec<Value>), String> {
        let cands: Vec<Value> = {
            let w = self.world.borrow();
            w.entities
                .iter()
                .filter(|(_, e)| kind.map_or(true, |k| e.kind == k))
                .filter(|(_, e)| {
                    near.map_or(true, |(x, y, r)| {
                        let (cx, cy) = e.center();
                        (cx - (x + 0.5)).abs().max((cy - (y + 0.5)).abs()) <= r + 1e-9
                    })
                })
                .map(|(id, e)| World::entity_json(*id, e))
                .collect()
        };
        let mut out = cands;
        if let Some(f) = filter {
            let ast = self.engine.compile_expression(f).map_err(|e| format!("where: {e}"))?;
            let mut first_err = None;
            out.retain(|v| {
                let mut scope = Scope::new();
                scope.push("e", to_dyn(v));
                match self.engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast) {
                    Ok(r) => r.as_bool().unwrap_or(false),
                    // e.g. `e.hp < 5` on an entity with no hp: treat as no match.
                    Err(err) => {
                        first_err.get_or_insert(err.to_string());
                        false
                    }
                }
            });
            if out.is_empty() {
                if let Some(err) = first_err {
                    return Err(format!("where: no matches; first error: {err}"));
                }
            }
        }
        let total = out.len();
        out.truncate(limit);
        Ok((total, out))
    }

    pub fn events_since(&self, seq: u64) -> Vec<Event> {
        self.world.borrow().events.iter().filter(|e| e.seq > seq).cloned().collect()
    }

    /// Runs `runs` playthroughs from a snapshot with different RNG seeds and scores each
    /// with `metric`. The live world is restored afterwards.
    pub fn trials(&mut self, from: &str, runs: u64, ticks: u64, until: Option<&str>, metric: &str, inputs: Option<&InputTimeline>) -> Result<Value, String> {
        let snap = self.snapshots.get(from).cloned().ok_or(format!("no snapshot '{from}'"))?;
        let saved = self.world.borrow().clone();
        self.recording = false;
        self.replace_world(snap.clone());
        let result = (|| {
            let mut out = vec![];
            for i in 0..runs {
                let mut w = snap.clone();
                w.rng = snap.rng ^ (i + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                *self.world.borrow_mut() = w;
                let start = self.world.borrow().event_seq;
                let (ran, stopped) = self.step(ticks, until, inputs)?;
                let metric = self.eval_expr(metric).map_err(|e| format!("metric: {e}"))?;
                let mut counts = BTreeMap::<String, u64>::new();
                for ev in self.events_since(start) {
                    *counts.entry(ev.kind).or_default() += 1;
                }
                out.push(json!({ "run": i, "ticks": ran, "until_hit": stopped, "metric": metric, "events": counts }));
            }
            Ok::<_, String>(out)
        })();
        self.replace_world(saved);
        self.recording = true;
        let out = result?;

        let nums: Vec<f64> = out.iter().filter_map(|r| r["metric"].as_f64()).collect();
        let trues = out.iter().filter(|r| r["metric"] == json!(true)).count();
        let ticks: Vec<u64> = out.iter().filter_map(|r| r["ticks"].as_u64()).collect();
        let mut summary = json!({ "runs": runs, "avg_ticks": ticks.iter().sum::<u64>() as f64 / ticks.len().max(1) as f64 });
        if !nums.is_empty() {
            summary["metric_min"] = json!(nums.iter().cloned().fold(f64::INFINITY, f64::min));
            summary["metric_max"] = json!(nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            summary["metric_mean"] = json!(nums.iter().sum::<f64>() / nums.len() as f64);
        } else {
            summary["metric_true"] = json!(trues);
        }
        Ok(json!({ "summary": summary, "runs": out }))
    }

    /// Everything a renderer needs for the current tick (editor and web player share this).
    pub fn frame(&self) -> Value {
        let w = self.world.borrow();
        frame_of(&w)
    }
}

/// The per-tick picture: entities, HUD, camera. The map is referenced by the host.
pub fn frame_of(w: &World) -> Value {
    json!({
        "tick": w.tick,
        "ents": physics::frame_entities(w),
        "hud": w.hud,
        "camera": {
            "x": num(w.camera.x), "y": num(w.camera.y), "view": w.camera.view,
            "zoom": w.camera.zoom, "shake": w.camera.shake,
        },
        "tile_size": w.tile_size,
        "tick_rate": w.tick_rate,
    })
}

