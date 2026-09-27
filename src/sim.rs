//! The simulation: a world plus a Rhai engine whose API reads and writes that world.
//! Entity scripts define `fn tick(me)`; an optional `rules` script defines `fn rules()`.

use crate::viewer::FeedRef;
use crate::world::{Event, World};
use rhai::{Array, CallFnOptions, Dynamic, Engine, EvalAltResult, Map as RMap, Scope, AST};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

type W = Rc<RefCell<World>>;
type RR<T> = Result<T, Box<EvalAltResult>>;

pub fn to_dyn(v: &Value) -> Dynamic {
    rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT)
}

pub fn to_json(d: &Dynamic) -> Value {
    rhai::serde::from_dynamic::<Value>(d).unwrap_or(Value::Null)
}

fn ents(w: &World, pred: impl Fn(&crate::world::Entity) -> bool) -> Array {
    w.entities
        .iter()
        .filter(|(_, e)| pred(e))
        .map(|(id, e)| to_dyn(&World::entity_json(*id, e)))
        .collect()
}

fn cheb(ax: i64, ay: i64, bx: i64, by: i64) -> i64 {
    (ax - bx).abs().max((ay - by).abs())
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
        w.entities
            .get(&(id as u64))
            .map_or(Dynamic::UNIT, |en| to_dyn(&World::entity_json(id as u64, en)))
    });
    reg!(e, w, "set", move |id: i64, key: &str, v: Dynamic| -> RR<()> {
        let mut w = w.borrow_mut();
        let en = w.entities.get_mut(&(id as u64)).ok_or_else(|| format!("set: no entity {id}"))?;
        match key {
            "id" => return Err("set: id is read-only".into()),
            "x" => en.x = v.as_int().map_err(|t| format!("set: x must be int, got {t}"))?,
            "y" => en.y = v.as_int().map_err(|t| format!("set: y must be int, got {t}"))?,
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
    reg!(e, w, "create", move |kind: &str, x: i64, y: i64| -> i64 {
        w.borrow_mut().spawn(kind, x, y, BTreeMap::new()) as i64
    });
    reg!(e, w, "create", move |kind: &str, x: i64, y: i64, props: RMap| -> i64 {
        let props = props.into_iter().map(|(k, v)| (k.to_string(), to_json(&v))).collect();
        w.borrow_mut().spawn(kind, x, y, props) as i64
    });
    reg!(e, w, "destroy", move |id: i64| -> bool {
        w.borrow_mut().entities.remove(&(id as u64)).is_some()
    });
    reg!(e, w, "find", move |kind: &str| -> Array { ents(&w.borrow(), |e| e.kind == kind) });
    reg!(e, w, "entities", move || -> Array { ents(&w.borrow(), |_| true) });
    reg!(e, w, "at", move |x: i64, y: i64| -> Array { ents(&w.borrow(), |e| e.x == x && e.y == y) });
    reg!(e, w, "tagged", move |tag: &str| -> Array {
        ents(&w.borrow(), |e| {
            e.props.get("tags").and_then(Value::as_array).is_some_and(|t| t.iter().any(|v| v.as_str() == Some(tag)))
        })
    });
    reg!(e, w, "count", move |kind: &str| -> i64 {
        w.borrow().entities.values().filter(|e| e.kind == kind).count() as i64
    });
    reg!(e, w, "near", move |x: i64, y: i64, r: i64| -> Array {
        let w = w.borrow();
        let mut v: Vec<_> = w.entities.iter().filter(|(_, en)| cheb(en.x, en.y, x, y) <= r).collect();
        v.sort_by_key(|(id, en)| (cheb(en.x, en.y, x, y), **id));
        v.into_iter().map(|(id, en)| to_dyn(&World::entity_json(*id, en))).collect()
    });

    // --- movement ---
    reg!(e, w, "move_by", move |id: i64, dx: i64, dy: i64| -> RR<bool> {
        let mut w = w.borrow_mut();
        let en = w.entities.get(&(id as u64)).ok_or_else(|| format!("move_by: no entity {id}"))?;
        let (x, y) = (en.x + dx, en.y + dy);
        if !w.walkable(x, y) {
            return Ok(false);
        }
        let en = w.entities.get_mut(&(id as u64)).unwrap();
        (en.x, en.y) = (x, y);
        Ok(true)
    });
    reg!(e, w, "move_toward", move |id: i64, tx: i64, ty: i64| -> RR<bool> {
        let mut w = w.borrow_mut();
        let en = w.entities.get(&(id as u64)).ok_or_else(|| format!("move_toward: no entity {id}"))?;
        let Some((x, y)) = w.next_step((en.x, en.y), (tx, ty)) else { return Ok(false) };
        let en = w.entities.get_mut(&(id as u64)).unwrap();
        (en.x, en.y) = (x, y);
        Ok(true)
    });
    reg!(e, w, "path_len", move |x1: i64, y1: i64, x2: i64, y2: i64| -> i64 {
        w.borrow().path_len((x1, y1), (x2, y2)).map_or(-1, |d| d as i64)
    });

    // --- map ---
    reg!(e, w, "tile", move |x: i64, y: i64| -> String { w.borrow().tile(x, y).to_string() });
    reg!(e, w, "set_tile", move |x: i64, y: i64, c: &str| -> bool {
        c.chars().next().is_some_and(|c| w.borrow_mut().set_tile(x, y, c))
    });
    reg!(e, w, "walkable", move |x: i64, y: i64| -> bool { w.borrow().walkable(x, y) });
    reg!(e, w, "width", move || -> i64 { w.borrow().width() });
    reg!(e, w, "height", move || -> i64 { w.borrow().height() });

    reg!(e, w, "create_prefab", move |name: &str, x: i64, y: i64| -> RR<i64> {
        Ok(w.borrow_mut().spawn_prefab(name, x, y, BTreeMap::new())? as i64)
    });
    reg!(e, w, "solid", move |x: i64, y: i64| -> bool { !w.borrow().walkable(x, y) });

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
    reg!(e, w, "emit", move |kind: &str| { w.borrow_mut().emit(kind, Value::Null) });
    reg!(e, w, "emit", move |kind: &str, data: Dynamic| { w.borrow_mut().emit(kind, to_json(&data)) });
    reg!(e, w, "state", move |name: &str| -> Dynamic {
        w.borrow().vars.get(name).map_or(Dynamic::UNIT, to_dyn)
    });
    reg!(e, w, "state", move |name: &str, default: Dynamic| -> Dynamic {
        w.borrow().vars.get(name).map_or(default, to_dyn)
    });
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
    /// Where each tick is recorded for the editor; paused during trials.
    pub feed: Option<FeedRef>,
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
            feed: None,
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
        if let (true, Some(feed)) = (self.recording, &self.feed) {
            feed.lock().unwrap().push(&self.world.borrow(), note, events, jump);
        }
    }

    pub fn run_tick(&mut self) {
        let start = self.world.borrow().event_seq;
        let jobs: Vec<(u64, String)> = self
            .world
            .borrow()
            .entities
            .iter()
            .filter_map(|(id, e)| e.script.clone().map(|s| (*id, s)))
            .collect();
        let opts = || CallFnOptions::new().eval_ast(false);
        for (id, script) in jobs {
            // An earlier script this tick may have destroyed it.
            if !self.world.borrow().entities.contains_key(&id) {
                continue;
            }
            let res = match self.asts.get(&script) {
                Some(ast) => self
                    .engine
                    .call_fn_with_options::<Dynamic>(opts(), &mut Scope::new(), ast, "tick", (id as i64,))
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                None => Err(format!("script '{script}' is missing or failed to compile")),
            };
            if let Err(msg) = res {
                self.report(json!({ "script": script, "entity": id, "error": msg }));
            }
        }
        if let Some(ast) = self.asts.get("rules") {
            if ast.iter_functions().any(|f| f.name == "rules" && f.params.is_empty()) {
                let res = self.engine.call_fn_with_options::<Dynamic>(opts(), &mut Scope::new(), ast, "rules", ());
                if let Err(e) = res {
                    self.report(json!({ "script": "rules", "error": e.to_string() }));
                }
            }
        }
        {
            let mut w = self.world.borrow_mut();
            w.tick += 1;
            w.end_tick_input();
        }
        if self.recording && self.feed.is_some() {
            let evs = self.events_since(start).into_iter().take(20).map(|e| json!({ "kind": e.kind, "data": e.data })).collect();
            self.record(None, evs, false);
        }
    }

    /// Runs up to `ticks` ticks, stopping early once the `until` expression is true.
    pub fn step(&mut self, ticks: u64, until: Option<&str>) -> Result<(u64, bool), String> {
        let until = until
            .map(|s| self.engine.compile_expression(s).map_err(|e| format!("until: {e}")))
            .transpose()?;
        for i in 0..ticks {
            self.run_tick();
            if let Some(ast) = &until {
                let v = self.engine.eval_ast::<Dynamic>(ast).map_err(|e| format!("until: {e}"))?;
                if v.as_bool().unwrap_or(false) {
                    return Ok((i + 1, true));
                }
            }
        }
        Ok((ticks, false))
    }

    pub fn exec(&mut self, code: &str) -> Result<Value, String> {
        self.engine
            .eval_with_scope::<Dynamic>(&mut self.repl, code)
            .map(|d| to_json(&d))
            .map_err(|e| e.to_string())
    }

    pub fn eval_expr(&self, expr: &str) -> Result<Value, String> {
        self.engine.eval_expression::<Dynamic>(expr).map(|d| to_json(&d)).map_err(|e| e.to_string())
    }

    /// Filters entities by kind, proximity and an arbitrary `where` expression over `e`.
    pub fn query(
        &self,
        kind: Option<&str>,
        near: Option<(i64, i64, i64)>,
        filter: Option<&str>,
        limit: usize,
    ) -> Result<(usize, Vec<Value>), String> {
        let cands: Vec<Value> = {
            let w = self.world.borrow();
            w.entities
                .iter()
                .filter(|(_, e)| kind.map_or(true, |k| e.kind == k))
                .filter(|(_, e)| near.map_or(true, |(x, y, r)| cheb(e.x, e.y, x, y) <= r))
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
    pub fn trials(
        &mut self,
        from: &str,
        runs: u64,
        ticks: u64,
        until: Option<&str>,
        metric: &str,
    ) -> Result<Value, String> {
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
                let (ran, stopped) = self.step(ticks, until)?;
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
        let mut summary = json!({
            "runs": runs,
            "avg_ticks": ticks.iter().sum::<u64>() as f64 / ticks.len().max(1) as f64,
        });
        if !nums.is_empty() {
            summary["metric_min"] = json!(nums.iter().cloned().fold(f64::INFINITY, f64::min));
            summary["metric_max"] = json!(nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            summary["metric_mean"] = json!(nums.iter().sum::<f64>() / nums.len() as f64);
        } else {
            summary["metric_true"] = json!(trues);
        }
        Ok(json!({ "summary": summary, "runs": out }))
    }
}
