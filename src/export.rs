//! Exporting a game as one self-contained HTML file: the web player, the renderer, the
//! engine compiled to WebAssembly and the world, all inlined. Double-click to play.

use crate::viewer::RENDER_JS;
use crate::world::World;
use std::path::{Path, PathBuf};

const PLAYER: &str = include_str!("web/player.html");

/// The WebAssembly build of the engine: next to the exe (installed app) or in the build output.
pub fn find_wasm() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidates = vec![exe.with_file_name("forge_web.wasm")];
    for d in exe.ancestors() {
        candidates.push(d.join("target").join("wasm32-unknown-unknown").join("release").join("forge_web.wasm"));
    }
    candidates.into_iter().find(|p| p.exists())
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn slug(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let t = t.trim_matches('-').to_string();
    if t.is_empty() { "game".into() } else { t }
}

/// Writes `<dir>/<title>.html` and returns its path.
pub fn export(w: &World, title: &str, dir: &Path) -> Result<PathBuf, String> {
    let wasm = find_wasm().ok_or("the web player isn't built yet: run install.ps1, which builds it (web/ for wasm32-unknown-unknown)")?;
    let bytes = std::fs::read(&wasm).map_err(|e| format!("reading {}: {e}", wasm.display()))?;
    // Inside a <script>, "</" must not appear literally.
    let world = w.to_bundle().to_string().replace("</", "<\\/");
    let html = PLAYER
        .replace("__TITLE__", &escape_html(title))
        .replace("/*__RENDER__*/", RENDER_JS)
        .replace("__WASM__", &base64(&bytes))
        // Last, so text inside the world can't be mistaken for a placeholder.
        .replace("\"__WORLD__\"", &world);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{}.html", slug(title)));
    std::fs::write(&file, html).map_err(|e| format!("writing {}: {e}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_known_values() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
