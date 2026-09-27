//! forge: an agent-native 2D game engine.
//!
//! Usage: `forge [world_dir] [--mcp] [--editor] [--port N] [--no-editor]`
//!
//! Plain mode: one JSON command per line on stdin, one JSON response per line on stdout.
//! Blank lines and lines starting with `#` are skipped, so a session can be a `.jsonl` file.
//! `--mcp` speaks MCP on stdio instead. Either way the editor is served on localhost
//! (in plain mode only with `--editor`, which also keeps serving after stdin ends).

mod app;
mod commands;
mod export;
mod mcp;
mod viewer;

// The core lives in the library crate; these keep `crate::world` etc. working here.
use forge::{physics, sim, world};

use app::App;
use serde_json::{json, Value};
use sim::Sim;
use std::io::{BufRead, Write};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, Instant};
use viewer::Msg;
use world::World;

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
}

/// 1 ms timer resolution on Windows, so 60 Hz play is smooth (the default is ~15.6 ms).
fn fine_timers() {
    #[cfg(windows)]
    unsafe {
        timeBeginPeriod(1);
    }
}

/// The project folder: the nearest ancestor of the exe (or the cwd) containing `worlds/`.
fn project_root() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.ancestors().find(|d| d.join("worlds").is_dir()).map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// This build's identity: the modified time of the running exe.
fn build_id() -> String {
    std::env::current_exe()
        .and_then(std::fs::metadata)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let mcp = flag("--mcp");
    let editor_only = flag("--editor");
    // Used by the Forge app: quit once its window has stopped polling the editor.
    let exit_when_closed = flag("--exit-when-closed");
    let started = Instant::now();
    let port: u16 = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(7878);
    let world_arg = args
        .iter()
        .enumerate()
        .find(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--port"))
        .map(|(_, a)| a.clone());

    fine_timers();
    let _ = physics::jump_speed(0.0, 0.0);
    let _ = viewer::EXPORTS_DIR.set(project_root().join("exports"));
    let mut app = App::new(Sim::new(World::blank(vec!["##########".into(), "#........#".into(), "##########".into()])));
    app.build = build_id();
    app.managed = exit_when_closed;
    app.autosave_enabled = !flag("--no-autosave");
    let feed: viewer::FeedRef = Arc::new(viewer::Shared::default());
    {
        let feed = feed.clone();
        app.sim.recorder = Some(Box::new(move |w, note, events, jump| {
            feed.feed.lock().unwrap().push(w, note, events, jump);
            feed.arrived.notify_all();
        }));
    }
    if let Some(p) = world_arg {
        if let Err(e) = app.load(&p) {
            eprintln!("forge: {e}");
            std::process::exit(1);
        }
    }
    app.sim.record(Some("engine started"), vec![], true);

    let (tx, rx) = std::sync::mpsc::channel::<Msg>();
    if (mcp || editor_only) && !flag("--no-editor") {
        app.editor_port = viewer::serve(feed, tx.clone(), port);
        match app.editor_url() {
            Some(url) => eprintln!("forge: editor at {url}"),
            None => eprintln!("forge: no free port for the editor"),
        }
    }
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if tx.send(Msg::Line(line)).is_err() {
                    return;
                }
            }
            let _ = tx.send(Msg::Eof);
        });
    }

    let mut out = std::io::stdout().lock();
    let mut next_tick = Instant::now();
    loop {
        let mut wait = if app.playing {
            next_tick.saturating_duration_since(Instant::now())
        } else {
            Duration::from_secs(3600)
        };
        // Wake up now and then to autosave.
        wait = wait.min(Duration::from_secs(5));
        app.autosave_tick();
        if exit_when_closed {
            wait = wait.min(Duration::from_secs(1));
            let gone = match app.editor_idle_secs() {
                Some(idle) => idle > 20,
                None => started.elapsed().as_secs() > 90,
            };
            if gone {
                break;
            }
        }
        match rx.recv_timeout(wait) {
            Ok(Msg::Line(line)) => {
                let t = line.trim();
                let resp = if mcp {
                    if t.is_empty() { None } else { mcp::handle_line(&mut app, t) }
                } else if t.is_empty() || t.starts_with('#') {
                    None
                } else {
                    Some(match serde_json::from_str::<Value>(t) {
                        Ok(c) => app.run(&c, "cli"),
                        Err(e) => json!({ "ok": false, "error": format!("bad json: {e}") }),
                    })
                };
                if let Some(r) = resp {
                    let _ = writeln!(out, "{r}");
                    let _ = out.flush();
                }
            }
            Ok(Msg::Cmd(c, reply)) => {
                // The editor may send one command or a batch.
                let r = match c.as_array() {
                    Some(cmds) => Value::Array(cmds.iter().map(|c| app.run(c, "you")).collect()),
                    None => app.run(&c, "you"),
                };
                let _ = reply.send(r);
                if app.quit {
                    std::thread::sleep(Duration::from_millis(200)); // let the reply go out
                    break;
                }
            }
            // With --editor, keep serving after stdin ends; otherwise stdin closing ends the engine.
            Ok(Msg::Eof) if editor_only => {}
            Ok(Msg::Eof) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if !app.playing {
            next_tick = Instant::now();
        } else {
            // Catch up a few ticks if the OS woke us late, so play speed stays true.
            let dt = Duration::from_secs_f64(1.0 / app.tps);
            let mut ran = 0;
            while app.playing && Instant::now() >= next_tick && ran < 4 {
                app.sim.run_tick();
                app.after_ticks();
                app.touch();
                next_tick += dt;
                ran += 1;
            }
            if Instant::now() > next_tick + dt * 8 {
                next_tick = Instant::now(); // far behind (e.g. a slow script): don't burst
            }
        }
    }
    app.autosave_now();
}
