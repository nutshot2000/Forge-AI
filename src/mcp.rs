//! Minimal MCP server over stdio. The whole command protocol is exposed as one
//! batched `forge` tool, so a single call can load, edit, simulate and inspect.

use crate::app::App;
use crate::commands;
use serde_json::{json, Value};

const INSTRUCTIONS: &str = "forge is a text-native 2D game engine, and you are the user's right arm in it. \
Use the `forge` tool with a batch of commands, batching as much as you can per call. Start with {\"cmd\":\"look\"}: \
it shows the map, what the user has selected or is hovering (so 'this' / 'here' resolve), assets and recent activity. \
Build with new/paint/tile/sprite/prefab/create/script; test with snapshot -> step/trials -> diff -> tweak; \
drive the editor with ui (select, open scripts, switch tools), talk with say, show places with point. Everything is undoable.";

fn tool_def() -> Value {
    let help = commands::help();
    let items: Vec<Value> = commands::COMMANDS.iter().map(commands::command_schema).collect();
    json!({
        "name": "forge",
        "description": format!(
            "Run a batch of forge engine commands in order against the live world the user is editing \
             (state persists between calls). Stops at the first failing command unless keep_going is true; \
             atomic makes the whole call all-or-nothing with one undo step. {}\nScript API (Rhai): {}",
            help["start_here"].as_str().unwrap_or(""),
            serde_json::to_string(&help["script_api"]).unwrap()
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "commands": {
                    "type": "array",
                    "description": "Command objects, run in order. Start with {\"cmd\":\"look\"}.",
                    "items": { "anyOf": items }
                },
                "keep_going": { "type": "boolean", "description": "Continue after a failed command (default false)." },
                "atomic": { "type": "boolean", "description": "All or nothing: if any command fails, undo the whole call. One undo step." }
            },
            "required": ["commands"]
        }
    })
}

/// One line per command result. `view` rows are printed as a real text block.
fn render(resp: &Value) -> String {
    let mut r = resp.clone();
    let Some(rows) = r.as_object_mut().and_then(|m| m.remove("rows")) else {
        return resp.to_string();
    };
    let grid: Vec<&str> = rows.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    format!("{r}\n{}", grid.join("\n"))
}

fn call(app: &mut App, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or("");
    if name != "forge" {
        return json!({ "isError": true, "content": [{ "type": "text", "text": format!("unknown tool '{name}'") }] });
    }
    let args = &params["arguments"];
    let Some(cmds) = args["commands"].as_array() else {
        return json!({ "isError": true, "content": [{ "type": "text", "text": "'commands' must be an array" }] });
    };
    let keep_going = args["keep_going"].as_bool().unwrap_or(false);
    if args["atomic"] == json!(true) {
        let r = app.run(&json!({ "cmd": "batch", "commands": cmds, "atomic": true }), "agent");
        let mut lines: Vec<String> = r["results"].as_array().into_iter().flatten().map(render).collect();
        lines.push(if r["ok"] == json!(true) { "(atomic: all applied, one undo step)".into() } else { format!("(atomic: rolled back: {})", r["error"].as_str().unwrap_or("")) });
        return json!({ "isError": r["ok"] != json!(true), "content": [{ "type": "text", "text": lines.join("\n") }] });
    }
    let mut lines = vec![];
    let mut images = vec![];
    let mut failed = false;
    for (i, c) in cmds.iter().enumerate() {
        let mut resp = app.run(c, "agent");
        // Screenshots go back as real images, not as base64 text.
        if let Some(img) = resp.as_object_mut().and_then(|m| m.remove("image")) {
            if let Some(b64) = img["base64"].as_str() {
                images.push(json!({ "type": "image", "data": b64, "mimeType": "image/png" }));
                resp["image"] = json!(format!("(attached as image {})", images.len()));
            }
        }
        let ok = resp["ok"] == json!(true);
        lines.push(render(&resp));
        if !ok {
            failed = true;
            if !keep_going && i + 1 < cmds.len() {
                lines.push(format!("(stopped: skipped {} remaining command(s))", cmds.len() - i - 1));
                break;
            }
        }
    }
    let mut content = vec![json!({ "type": "text", "text": lines.join("\n") })];
    content.extend(images);
    json!({ "isError": failed, "content": content })
}

/// Handles one JSON-RPC line; returns the response to send, if any.
pub fn handle_line(app: &mut App, line: &str) -> Option<Value> {
    let Ok(msg) = serde_json::from_str::<Value>(line) else {
        return Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } }));
    };
    // Notifications (no id) and responses need no reply.
    let (Some(id), Some(method)) = (msg.get("id").cloned(), msg["method"].as_str()) else {
        return None;
    };
    let reply = match method {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "forge", "version": env!("CARGO_PKG_VERSION") },
            "instructions": match app.editor_url() {
                Some(url) => format!("{INSTRUCTIONS} The user can watch and edit the same world live in the forge editor at {url}; everything you do shows up there, tagged as agent."),
                None => INSTRUCTIONS.to_string(),
            },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": [tool_def()] })),
        "tools/call" => Ok(call(app, &msg["params"])),
        _ => Err(json!({ "code": -32601, "message": format!("method not found: {method}") })),
    };
    Some(match reply {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}
