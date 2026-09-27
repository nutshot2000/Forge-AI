//! Live viewer: the sim records a compact frame per tick into a shared feed, and a tiny
//! HTTP server (std only) serves a canvas page that replays those frames in a browser.

use crate::world::World;
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_FRAMES: usize = 5000;
const MAX_PER_RESPONSE: usize = 1000;
const PAGE: &str = include_str!("viewer.html");

#[derive(Default)]
pub struct Feed {
    seq: u64,
    frames: VecDeque<Value>,
    /// Maps by content hash; frames reference them so tile edits replay correctly.
    maps: HashMap<String, Vec<String>>,
}

pub type FeedRef = Arc<Mutex<Feed>>;

impl Feed {
    pub fn push(&mut self, w: &World, note: Option<&str>, events: Vec<Value>, jump: bool) {
        let mut h = DefaultHasher::new();
        w.map.hash(&mut h);
        let key = format!("{:x}", h.finish());
        self.maps.entry(key.clone()).or_insert_with(|| w.map.clone());
        self.seq += 1;
        let ents: Vec<Value> = w
            .entities
            .iter()
            .map(|(id, e)| {
                let color = e.props.get("color").cloned().unwrap_or(Value::Null);
                let sprite = e.props.get("sprite").cloned().unwrap_or(Value::Null);
                json!([id, e.glyph().to_string(), e.x, e.y, e.kind, color, sprite])
            })
            .collect();
        self.frames.push_back(json!({
            "seq": self.seq, "tick": w.tick, "map": key, "note": note, "jump": jump, "ents": ents, "events": events, "hud": w.hud,
        }));
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
        let frames: Vec<&Value> = self
            .frames
            .iter()
            .filter(|f| f["seq"].as_u64().unwrap_or(0) > after)
            .take(MAX_PER_RESPONSE)
            .collect();
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

fn handle(mut stream: TcpStream, feed: &FeedRef, tx: &Sender<Msg>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
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
    let (status, ctype, body) = if path == "/" {
        ("200 OK", "text/html; charset=utf-8", PAGE.to_string())
    } else if let Some(q) = path.strip_prefix("/frames") {
        let after = q
            .trim_start_matches('?')
            .split('&')
            .find_map(|kv| kv.strip_prefix("after="))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let body = feed.lock().unwrap().since(after).to_string();
        ("200 OK", "application/json", body)
    } else if method == "POST" && path == "/api/cmd" {
        let mut buf = vec![0u8; len.min(4 << 20)];
        reader.read_exact(&mut buf)?;
        let resp = match serde_json::from_slice::<Value>(&buf) {
            Ok(cmd) => {
                let (rtx, rrx) = std::sync::mpsc::channel();
                let _ = tx.send(Msg::Cmd(cmd, rtx));
                rrx.recv_timeout(Duration::from_secs(60))
                    .unwrap_or_else(|_| json!({ "ok": false, "error": "engine did not respond" }))
            }
            Err(e) => json!({ "ok": false, "error": format!("bad json: {e}") }),
        };
        ("200 OK", "application/json", resp.to_string())
    } else {
        ("404 Not Found", "text/plain", "not found".to_string())
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
