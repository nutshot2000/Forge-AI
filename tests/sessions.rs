//! Replays every `tests/sessions/*.jsonl` through the forge binary. Each line is a command;
//! an optional `"expect"` field is checked against the response as a subset match:
//!   - objects: every expected key must match (extra keys in the response are fine)
//!   - `"*"` matches anything that is present
//!   - `{"$contains": "text"}` matches a string containing text
//!   - `{"$len": n}` matches an array or object with n items
//! `$TMP` in any string is replaced with a fresh temp folder for the session.

use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn check(expect: &Value, got: &Value, path: &str) -> Result<(), String> {
    if expect == "*" {
        return if got.is_null() { Err(format!("{path}: expected a value, got nothing")) } else { Ok(()) };
    }
    if let Value::Object(e) = expect {
        if let Some(needle) = e.get("$contains").and_then(Value::as_str) {
            return match got.as_str() {
                Some(s) if s.contains(needle) => Ok(()),
                _ => Err(format!("{path}: expected text containing {needle:?}, got {got}")),
            };
        }
        if let Some(n) = e.get("$len").and_then(Value::as_u64) {
            let len = got.as_array().map(Vec::len).or(got.as_object().map(|o| o.len()));
            return if len == Some(n as usize) { Ok(()) } else { Err(format!("{path}: expected {n} items, got {got}")) };
        }
        let g = got.as_object().ok_or(format!("{path}: expected an object, got {got}"))?;
        for (k, v) in e {
            check(v, g.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"))?;
        }
        return Ok(());
    }
    if let (Value::Array(e), Value::Array(g)) = (expect, got) {
        if e.len() != g.len() {
            return Err(format!("{path}: expected {} items, got {}: {got}", e.len(), g.len()));
        }
        for (i, (ev, gv)) in e.iter().zip(g).enumerate() {
            check(ev, gv, &format!("{path}[{i}]"))?;
        }
        return Ok(());
    }
    // Numbers compare by value, so 3 matches 3.0.
    if let (Some(a), Some(b)) = (expect.as_f64(), got.as_f64()) {
        if a == b {
            return Ok(());
        }
    }
    if expect == got { Ok(()) } else { Err(format!("{path}: expected {expect}, got {got}")) }
}

fn run_session(file: &Path) -> Vec<String> {
    let tmp = std::env::temp_dir().join(format!("forge-test-{}-{}", std::process::id(), file.file_stem().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp_str = tmp.display().to_string().replace('\\', "/");

    let text = std::fs::read_to_string(file).unwrap().replace("$TMP", &tmp_str);
    let mut cmds = vec![];
    let mut expects = vec![];
    for (n, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut v: Value = serde_json::from_str(t).unwrap_or_else(|e| panic!("{}:{}: bad json: {e}", file.display(), n + 1));
        let expect = v.as_object_mut().unwrap().remove("expect");
        cmds.push(v.to_string());
        expects.push((n + 1, expect));
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("--no-autosave")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start forge");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all((cmds.join("\n") + "\n").as_bytes()).unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let responses: Vec<Value> = stdout.lines().map(|l| serde_json::from_str(l).unwrap()).collect();

    let mut failures = vec![];
    if responses.len() != cmds.len() {
        failures.push(format!("{}: sent {} commands, got {} responses", file.display(), cmds.len(), responses.len()));
    }
    for ((line, expect), resp) in expects.iter().zip(&responses) {
        if let Some(e) = expect {
            if let Err(msg) = check(e, resp, "") {
                failures.push(format!("{}:{line}: {msg}\n    response: {}", file.display(), trim(resp)));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    failures
}

fn trim(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 400 { format!("{}…", &s[..400]) } else { s }
}

#[test]
fn sessions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("sessions");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl")).collect();
    files.sort();
    assert!(!files.is_empty(), "no sessions in {}", dir.display());
    let failures: Vec<String> = files.iter().flat_map(|f| run_session(f)).collect();
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
}
