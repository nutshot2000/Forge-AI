//! The editor host: the sim records a frame per tick into a shared feed, and a tiny HTTP
//! server (std only) serves the editor page, streams frames to it live (server-sent
//! events), and accepts commands.

use crate::sim::frame_of;
use crate::world::World;
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

const MAX_FRAMES: usize = 6000;
const MAX_PER_RESPONSE: usize = 1000;
const PAGE: &str = include_str!("viewer.html");
pub const RENDER_JS: &str = include_str!("web/render.js");

/// Where exported games are written; served at /exports/.
pub static EXPORTS_DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Default)]
pub struct Feed {
    seq: u64,
    frames: VecDeque<Value>,
    /// Maps by content hash; frames reference them so tile edits replay correctly.
    maps: HashMap<String, Vec<String>>,
}

#[derive(Default)]
pub struct Shared {
    pub feed: Mutex<Feed>,
    /// Wakes live streams when a frame arrives.
    pub arrived: Condvar,
}

pub type FeedRef = Arc<Shared>;

pub fn map_key(map: &[String]) -> String {
    let mut h = DefaultHasher::new();
    map.hash(&mut h);
    format!("{:x}", h.finish())
}

impl Feed {
    pub fn push(&mut self, w: &World, note: Option<&str>, events: Vec<Value>, jump: bool) {
        let key = map_key(&w.map);
        self.maps.entry(key.clone()).or_insert_with(|| w.map.clone());
        self.seq += 1;
        let mut f = frame_of(w);
        f["seq"] = json!(self.seq);
        f["map"] = json!(key);
        f["note"] = json!(note);
        f["jump"] = json!(jump);
        f["events"] = json!(events);
        self.frames.push_back(f);
        if self.frames.len() > MAX_FRAMES {
            self.frames.pop_front();
            if self.maps.len() > 64 {
                let used: HashSet<&str> = self.frames.iter().filter_map(|f| f["map"].as_str()).collect();
                let keep: HashSet<String> = used.into_iter().map(String::from).collect();
                self.maps.retain(|k, _| keep.contains(k));
            }
        }
    }

    fn since(&self, after: u64) -> Value {
        let frames: Vec<&Value> =
            self.frames.iter().filter(|f| f["seq"].as_u64().unwrap_or(0) > after).take(MAX_PER_RESPONSE).collect();
        let mut maps = Map::new();
        for f in &frames {
            if let Some(k) = f["map"].as_str() {
                if let Some(rows) = self.maps.get(k) {
                    maps.insert(k.to_string(), json!(rows));
                }
            }
        }
        json!({ "latest": self.seq, "frames": frames, "maps": maps })
    }
}

/// Messages into the engine's main loop, which owns the world.
pub enum Msg {
    /// A line from stdin (MCP or plain protocol).
    Line(String),
    /// stdin closed.
    Eof,
    /// A command from the editor, with a channel for its response.
    Cmd(Value, Sender<Value>),
}

/// Binds the first free port from `port` upward and serves in background threads.
pub fn serve(feed: FeedRef, tx: Sender<Msg>, port: u16) -> Option<u16> {
    for p in port..port.saturating_add(10) {
        let Ok(listener) = TcpListener::bind(("127.0.0.1", p)) else { continue };
        let (feed, tx) = (feed.clone(), tx.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (feed, tx) = (feed.clone(), tx.clone());
                std::thread::spawn(move || {
                    let _ = handle(stream, &feed, &tx);
                });
            }
        });
        return Some(p);
    }
    None
}

fn query_u64(q: &str, key: &str) -> Option<u64> {
    q.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
}

/// Server-sent events: pushes frames to the editor as soon as the engine records them.
fn stream(mut out: TcpStream, feed: &FeedRef, mut after: u64) -> std::io::Result<()> {
    out.set_nodelay(true)?;
    write!(out, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n")?;
    out.flush()?;
    loop {
        let batch = {
            let mut f = feed.feed.lock().unwrap();
            if f.seq <= after {
                f = feed.arrived.wait_timeout(f, Duration::from_secs(10)).unwrap().0;
            }
            if f.seq < after {
                after = 0; // the engine's feed restarted
            }
            (f.seq > after).then(|| f.since(after))
        };
        match batch {
            Some(b) => {
                if let Some(last) = b["frames"].as_array().and_then(|a| a.last()).and_then(|f| f["seq"].as_u64()) {
                    after = last;
                }
                write!(out, "data: {b}\n\n")?;
            }
            None => write!(out, ": keep-alive\n\n")?,
        }
        out.flush()?;
    }
}

fn handle(mut stream_: TcpStream, feed: &FeedRef, tx: &Sender<Msg>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream_.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut len = 0usize;
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        if let Some((k, v)) = header.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        header.clear();
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or("GET"), parts.next().unwrap_or("/"));
    if let Some(q) = path.strip_prefix("/stream") {
        return stream(stream_, feed, query_u64(q, "after").unwrap_or(0));
    }
    let (status, ctype, body): (&str, &str, Vec<u8>) = if path == "/" {
        ("200 OK", "text/html; charset=utf-8", PAGE.as_bytes().to_vec())
    } else if path == "/render.js" {
        ("200 OK", "text/javascript; charset=utf-8", RENDER_JS.as_bytes().to_vec())
    } else if let Some(q) = path.strip_prefix("/frames") {
        let body = feed.feed.lock().unwrap().since(query_u64(q, "after").unwrap_or(0)).to_string();
        ("200 OK", "application/json", body.into_bytes())
    } else if let Some(name) = path.strip_prefix("/exports/") {
        let name = percent_decode(name);
        let file = EXPORTS_DIR.get().map(|d| d.join(&name));
        match file.filter(|_| !name.contains("..") && !name.contains('/') && !name.contains('\\')).and_then(|f| std::fs::read(f).ok()) {
            Some(bytes) => ("200 OK", "text/html; charset=utf-8", bytes),
            None => ("404 Not Found", "text/plain", b"no such export".to_vec()),
        }
    } else if method == "POST" && path == "/api/cmd" {
        let mut buf = vec![0u8; len.min(64 << 20)];
        reader.read_exact(&mut buf)?;
        let resp = match serde_json::from_slice::<Value>(&buf) {
            Ok(cmd) => {
                let (rtx, rrx) = std::sync::mpsc::channel();
                let _ = tx.send(Msg::Cmd(cmd, rtx));
                rrx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| json!({ "ok": false, "error": "engine did not respond" }))
            }
            Err(e) => json!({ "ok": false, "error": format!("bad json: {e}") }),
        };
        ("200 OK", "application/json", resp.to_string().into_bytes())
    } else {
        ("404 Not Found", "text/plain", b"not found".to_vec())
    };
    write!(
        stream_,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream_.write_all(&body)?;
    stream_.flush()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
