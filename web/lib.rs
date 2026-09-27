//! The forge engine as a WebAssembly module with a tiny C ABI, used by exported games.
//! Strings cross the boundary as UTF-8 bytes; results come back packed as
//! `(pointer << 32) | length` pointing into this module's memory.

use forge::sim::{frame_of, Sim};
use forge::world::World;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

thread_local! {
    static GAME: RefCell<Option<Sim>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERR: RefCell<String> = const { RefCell::new(String::new()) };
    static SEEN: RefCell<u64> = const { RefCell::new(0) };
}

fn out(s: String) -> u64 {
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        *o = s.into_bytes();
        ((o.as_ptr() as u64) << 32) | o.len() as u64
    })
}

/// Takes ownership of bytes the host wrote into memory from `fg_alloc`.
unsafe fn take(p: *mut u8, n: usize) -> String {
    String::from_utf8(Vec::from_raw_parts(p, n, n)).unwrap_or_default()
}

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

#[no_mangle]
pub extern "C" fn fg_alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Loads a world bundle (JSON). Returns 1 on success, 0 on failure (see `fg_error`).
///
/// # Safety
/// `p` must come from `fg_alloc(n)` and hold `n` bytes.
#[no_mangle]
pub unsafe extern "C" fn fg_load(p: *mut u8, n: usize) -> u32 {
    let text = take(p, n);
    let loaded = serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()).and_then(|v| World::from_bundle(&v));
    match loaded {
        Ok(mut w) => {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            w.level = v["level_name"].as_str().unwrap_or("").to_string();
            let mut sim = Sim::new(w);
            for (name, lv) in v["levels"].as_object().into_iter().flatten() {
                if let Ok(mut lw) = World::from_bundle(lv) {
                    lw.level = name.clone();
                    sim.levels.insert(name.clone(), lw);
                }
            }
            if let Some((name, err)) = sim.compile_errors.iter().next() {
                ERR.with(|e| *e.borrow_mut() = format!("script '{name}' has an error: {err}"));
            }
            GAME.with(|g| *g.borrow_mut() = Some(sim));
            1
        }
        Err(e) => {
            ERR.with(|x| *x.borrow_mut() = e);
            0
        }
    }
}

/// Keyboard: `{"down": [..], "up": [..], "clear": true}`.
///
/// # Safety
/// `p` must come from `fg_alloc(n)` and hold `n` bytes.
#[no_mangle]
pub unsafe extern "C" fn fg_input(p: *mut u8, n: usize) {
    let text = take(p, n);
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return };
    GAME.with(|g| {
        if let Some(sim) = g.borrow().as_ref() {
            let mut w = sim.world.borrow_mut();
            if v["clear"] == json!(true) {
                w.input = Default::default();
            }
            for k in v["down"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                let k = norm_key(k);
                if w.input.down.insert(k.clone()) {
                    w.input.pressed.insert(k);
                }
            }
            for k in v["up"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                w.input.down.remove(&norm_key(k));
            }
            let pair = |k: &str| v[k].as_array().and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]));
            if let Some(m) = pair("mouse") {
                w.input.mouse = Some(m);
            }
            if let Some(m) = pair("mouse_ui") {
                w.input.mouse_ui = Some(m);
            }
            for b in v["mouse_down"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                if w.input.buttons.insert(b.to_string()) {
                    w.input.buttons_pressed.insert(b.to_string());
                }
            }
            for b in v["mouse_up"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                w.input.buttons.remove(b);
            }
            if let Some(name) = v["ui_click"].as_str() {
                w.input.ui_clicks.insert(name.to_string());
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn fg_tick(n: u32) {
    GAME.with(|g| {
        if let Some(sim) = g.borrow_mut().as_mut() {
            for _ in 0..n {
                sim.run_tick();
            }
        }
    });
}

fn map_key(map: &[String]) -> String {
    let mut h = DefaultHasher::new();
    map.hash(&mut h);
    format!("{:x}", h.finish())
}

/// The current frame (same shape the editor draws), with `map` as a key; see `fg_map`.
#[no_mangle]
pub extern "C" fn fg_frame() -> u64 {
    let s = GAME.with(|g| {
        g.borrow().as_ref().map(|sim| {
            let w = sim.world.borrow();
            let mut f = frame_of(&w);
            f["map"] = json!(map_key(&w.map));
            // Sounds triggered since the previous frame.
            let since = SEEN.with(|s| *s.borrow());
            let sfx: Vec<Value> = w.events.iter().filter(|e| e.seq > since && e.kind == "sfx").map(|e| e.data["name"].clone()).collect();
            let fx: Vec<Value> = w.events.iter().filter(|e| e.seq > since && e.kind == "fx").map(|e| e.data.clone()).collect();
            SEEN.with(|s| *s.borrow_mut() = w.event_seq);
            f["sfx"] = json!(sfx);
            f["fx"] = json!(fx);
            f.to_string()
        })
    });
    out(s.unwrap_or_else(|| "null".into()))
}

/// The current map rows as a JSON array.
#[no_mangle]
pub extern "C" fn fg_map() -> u64 {
    let s = GAME.with(|g| g.borrow().as_ref().map(|sim| json!(sim.world.borrow().map).to_string()));
    out(s.unwrap_or_else(|| "[]".into()))
}

#[no_mangle]
pub extern "C" fn fg_error() -> u64 {
    out(ERR.with(|e| e.borrow().clone()))
}
