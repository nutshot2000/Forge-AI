//! Double-clickable launcher: starts the forge engine hidden in the background and opens
//! the editor in its own app window (Edge app mode). The engine stops by itself once the
//! window has been closed (it notices the editor has stopped polling).
//! If a forge engine is already running (e.g. one an AI agent is driving), it just opens
//! a window onto that one.
#![windows_subsystem = "windows"]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PORT: u16 = 7878;

fn forge_running(port: u16) -> bool {
    let addr = ("127.0.0.1", port);
    let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&addr) else { return false };
    addrs.into_iter().any(|a| TcpStream::connect_timeout(&a, Duration::from_millis(300)).is_ok())
}

/// Sends one command to a running engine's editor API.
fn engine_cmd(port: u16, body: &str) -> Option<serde_json::Value> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    write!(s, "POST /api/cmd HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).ok()?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).ok()?;
    serde_json::from_str(resp.split("\r\n\r\n").nth(1)?).ok()
}

fn exe_build(path: &Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// If the engine on `port` was started by this app from an older build, ask it to quit.
/// Engines an agent is driving are left alone.
fn retire_stale_engine(port: u16, installed: &Path) {
    let Some(state) = engine_cmd(port, r#"{"cmd":"state"}"#) else { return };
    let build = state["build"].as_str().unwrap_or("");
    let managed = state["managed"].as_bool().unwrap_or(false);
    if !managed || build == exe_build(installed) {
        return;
    }
    let _ = engine_cmd(port, r#"{"cmd":"quit"}"#);
    for _ in 0..50 {
        if !forge_running(port) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The forge project folder: the nearest ancestor of this exe that has worlds/.
fn project_dir(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find(|d| d.join("worlds").is_dir()).map(Path::to_path_buf)
}

fn edge() -> Option<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .iter()
        .filter_map(|v| std::env::var_os(v))
        .map(|base| PathBuf::from(base).join(r"Microsoft\Edge\Application\msedge.exe"))
        .find(|p| p.exists())
}

fn message(text: &str) {
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-Command", &format!(
            "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{}', 'Forge')",
            text.replace('\'', "''")
        )])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn main() {
    let exe = std::env::current_exe().expect("exe path");
    let engine_exe = exe.with_file_name("forge.exe");
    let root = project_dir(&exe);
    let world = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| {
            let worlds = root.as_ref()?.join("worlds");
            ["coin-dash", "dungeon"].iter().map(|w| worlds.join(w)).find(|p| p.join("world.json").exists())
        });

    let mut url = format!("http://127.0.0.1:{PORT}");
    let mut engine = None;
    if forge_running(PORT) {
        retire_stale_engine(PORT, &engine_exe);
    }
    if !forge_running(PORT) {
        let mut cmd = Command::new(&engine_exe);
        if let Some(w) = &world {
            cmd.arg(w);
        }
        cmd.args(["--editor", "--exit-when-closed"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(r) = &root {
            cmd.current_dir(r);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return message(&format!("Couldn't start the forge engine ({}): {e}", engine_exe.display())),
        };
        // The engine announces its editor URL (it may pick another port if 7878 is taken).
        let mut first = String::new();
        let mut err = BufReader::new(child.stderr.take().unwrap());
        let _ = err.read_line(&mut first);
        match first.trim().strip_prefix("forge: editor at ") {
            Some(u) => url = u.to_string(),
            None => {
                let _ = child.kill();
                return message(&format!("The forge engine failed to start:\n{}", first.trim()));
            }
        }
        std::thread::spawn(move || for _ in err.lines() {});
        engine = Some(child);
    }

    let profile = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("forge").join("window");
    let opened = edge().is_some_and(|edge| {
        Command::new(edge)
            .arg(format!("--app={url}"))
            .arg(format!("--user-data-dir={}", profile.display()))
            // Keep the editor polling while minimized, so the engine doesn't think it was closed.
            .args([
                "--window-size=1440,900",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-timer-throttling",
                "--disable-renderer-backgrounding",
                "--disable-backgrounding-occluded-windows",
            ])
            .spawn()
            .is_ok()
    });
    if !opened {
        let _ = Command::new("cmd").args(["/C", "start", "", &url]).creation_flags(CREATE_NO_WINDOW).status();
    }
    // The engine outlives this launcher and exits on its own when the window goes away.
    drop(engine);
}
