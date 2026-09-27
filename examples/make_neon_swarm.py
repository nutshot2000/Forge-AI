"""Builds worlds/neon-swarm (a mouse-aimed arena shooter) entirely through forge commands.

    python examples/make_neon_swarm.py > examples/neon-swarm-build.jsonl
    target/release/forge.exe --no-autosave < examples/neon-swarm-build.jsonl
"""
import json
import random

W, H = 32, 20
g = [['.'] * W for _ in range(H)]
for x in range(W):
    g[0][x] = g[H - 1][x] = '#'
for y in range(H):
    g[y][0] = g[y][W - 1] = '#'
for (x, y, w, h) in [(7, 5, 2, 2), (23, 5, 2, 2), (7, 13, 2, 2), (23, 13, 2, 2), (15, 3, 2, 1), (15, 16, 2, 1)]:
    for yy in range(y, y + h):
        for xx in range(x, x + w):
            g[yy][xx] = '#'
rows = [''.join(r) for r in g]

cmds = []
c = cmds.append
c({"cmd": "new", "map": rows, "seed": 11})
c({"cmd": "physics", "preset": "topdown"})
c({"cmd": "camera", "tile_size": 16})

# --- art ---
def sprite(name, palette, pixels, **kw):
    c({"cmd": "sprite", "name": name, "palette": palette, "pixels": pixels, **kw})

c({"cmd": "sprite", "name": "ship", "palette": {"b": "#2a7fff", "c": "#5ef2ff", "w": "#ffffff", "o": "#ff9f43", "y": "#ffe066"},
   "pixels": ["bb......", ".bcc....", ".bccccc.", "obccwwcc", "obccwwcc", ".bccccc.", ".bcc....", "bb......"],
   "frames": [["bb......", ".bcc....", ".bccccc.", "ybccwwcc", "ybccwwcc", ".bccccc.", ".bcc....", "bb......"]], "fps": 12})
sprite("drone", {"r": "#ff4d6d", "d": "#a3243d", "y": "#ffd060"},
       ["..rrrr..", ".rrrrrr.", "rrdyydrr", "rrdyydrr", "rrrrrrrr", ".rddddr.", "r.r..r.r", "........"],
       frames=[["..rrrr..", ".rrrrrr.", "rrdyydrr", "rrdyydrr", "rrrrrrrr", ".rddddr.", ".r.rr.r.", "........"]], fps=6)
sprite("brute", {"p": "#9b5cff", "m": "#5a2fa8", "y": "#ffd060"},
       ["pppppppp", "pmmmmmmp", "pmyppymp", "pmmmmmmp", "pmppppmp", "pmmmmmmp", "pp.pp.pp", "pp.pp.pp"])
sprite("sniper", {"g": "#5fd39a", "d": "#2f8a5a", "w": "#eafff4"},
       ["...gg...", "..gddg..", ".gdwwdg.", "gdwddwdg", "gdwddwdg", ".gdwwdg.", "..gddg..", "...gg..."],
       frames=[["...gg...", "..gddg..", ".gdddgg.", "gddwwddg", "gddwwddg", ".gdddgg.", "..gddg..", "...gg..."]], fps=4)
sprite("shot", {"w": "#baffff", "c": "#5ef2ff"}, [".cc.", "cwwc", "cwwc", ".cc."])
sprite("ebullet", {"r": "#ff6b8a", "w": "#fff0f3"}, [".rr.", "rwwr", "rwwr", ".rr."])
sprite("wall", {"b": "#3a4bd8", "d": "#151a3d", "l": "#6b7cff"},
       ["lbbbbbbl", "bddddddb", "bddddddb", "bddddddb", "bddddddb", "bddddddb", "bddddddb", "lbbbbbbl"])
rnd = random.Random(4)
stars = []
for y in range(32):
    row = ""
    for x in range(64):
        r = rnd.random()
        row += "s" if r < 0.012 else ("t" if r < 0.035 else ".")
    stars.append(row)
sprite("stars", {"s": "#c9d6ff", "t": "#4b5a9a"}, stars)
c({"cmd": "tile", "char": "#", "name": "neon wall", "solid": True, "sprite": "wall"})
c({"cmd": "background", "sky": ["#05060f", "#0d1233", "#1a1040"], "layers": [
    {"sprite": "stars", "parallax": 0.15, "y": 0, "height": 10},
    {"sprite": "stars", "parallax": 0.3, "y": 10, "height": 10},
]})

# --- sound ---
c({"cmd": "sound", "name": "laser", "preset": "laser", "vol": 0.08})
c({"cmd": "sound", "name": "boom", "preset": "explosion", "vol": 0.3})
c({"cmd": "sound", "name": "hit", "preset": "hit", "vol": 0.15})
c({"cmd": "sound", "name": "wave", "preset": "powerup"})
c({"cmd": "sound", "name": "blip", "preset": "blip"})
c({"cmd": "sound", "name": "powerup", "preset": "powerup"})

# --- behaviour (library + a few game-specific scripts) ---
for b in ["topdown_player", "health", "chaser", "shooter", "projectile"]:
    c({"cmd": "use_behavior", "name": b})

c({"cmd": "script", "name": "gunner", "code": """// The player's gun: aims at the mouse, fires while the left button (or Space) is held.
fn tick(me) {
    if !state("started", false) || state("over", false) { return; }
    let mx = mouse_x();
    if mx >= 0.0 { set(me, "angle", angle_to_point(me, mx, mouse_y())); }
    let cd = prop(me, "cooldown", 0);
    if cd > 0 { set(me, "cooldown", cd - 1); return; }
    if mouse_down("left") || key("space") {
        let c = center(me);
        let a = prop(me, "angle", 0.0);
        let v = vel_from_angle(a, 18.0);
        let b = create_prefab("shot", c[0] - 0.125 + v[0] * 0.03, c[1] - 0.125 + v[1] * 0.03, #{ from: me, angle: a });
        set_vel(b, v[0], v[1]);
        set(me, "cooldown", 6);
        sfx("laser");
    }
}
"""})
c({"cmd": "script", "name": "enemy", "code": """// Enemies: take damage (flash + number), explode and score when destroyed, and hurt the
// player by ramming them.
fn on_message(me, msg, data) {
    if msg != "damage" { return; }
    let amount = if type_of(data) == "map" && "amount" in data { data.amount } else { 1 };
    let hp = prop(me, "hp", 1) - amount;
    let c = center(me);
    float_text(c[0], c[1] - 0.5, "-" + amount, "#ffd060");
    if hp > 0 {
        set(me, "hp", hp);
        set(me, "alpha", 0.35);
        tween(me, "alpha", 1.0, 8);
        sfx("hit");
        return;
    }
    particles("explosion", c[0], c[1]);
    camera_shake(0.2);
    freeze(2);
    sfx("boom");
    let pts = prop(me, "points", 10);
    set_game("score", game("score", 0) + pts);
    float_text(c[0], c[1], "+" + pts, "#5fd39a");
    destroy(me);
}

fn on_touch(me, other) {
    if has_tag(other.id, "player") && !state("over", false) {
        send(other.id, "damage", #{ amount: 1 });
        let c = center(me);
        particles("explosion", c[0], c[1], #{ color: "#ff5f6d", count: 20 });
        camera_shake(0.5);
        flash(10, "#ff3048");
        destroy(me);
    }
}
"""})
c({"cmd": "script", "name": "rules", "code": """// Title screen, waves, HUD, game over.
fn rules() {
    if !state("started", false) {
        if !state("title_shown", false) {
            set_state("title_shown", true);
            set_state("paused", true);
            show_screen("NEON SWARM", "Move: WASD / arrows    Aim: mouse    Fire: hold left click", "");
            ui("start", #{ type: "button", text: "START", x: 50, y: 72, anchor: "center", size: 5 });
        }
        return;
    }
    let players = tagged("player");
    if players.is_empty() { return; }
    let p = players[0];
    ui_set("hp", "value", max(0, p.hp));
    ui_set("score", "text", "SCORE " + game("score", 0));
    ui_set("wave", "text", "WAVE " + state("wave", 0));
    if state("over", false) {
        if !state("restart_shown", false) {
            set_state("restart_shown", true);
            ui("restart", #{ type: "button", text: "PLAY AGAIN", x: 50, y: 72, anchor: "center", size: 5 });
            ui("final", #{ type: "text", text: "Score " + game("score", 0) + "  ·  Wave " + state("wave", 0), x: 50, y: 58, anchor: "center", size: 4, color: "#7ee8ff" });
        }
        return;
    }
    // When the arena is clear, the next wave comes.
    if count("drone") + count("brute") + count("sniper") == 0 && !state("wave_pending", false) {
        set_state("wave_pending", true);
        after(-1, 60, "wave");
    }
}

fn on_timer(name, data) {
    if name == "wave" {
        let wave = state("wave", 0) + 1;
        set_state("wave", wave);
        set_state("wave_pending", false);
        ui("banner", #{ type: "text", text: "WAVE " + wave, x: 50, y: 40, anchor: "center", size: 9, color: "#7ee8ff", font: "mono" });
        after(-1, 90, "banner_off");
        sfx("wave");
        let n = 3 + wave * 2;
        for i in 0..n { spawn_enemy(wave, i); }
    }
    if name == "banner_off" { ui_remove("banner"); }
}

fn spawn_enemy(wave, i) {
    // Around the edges of the arena.
    let side = rand(4);
    let x = if side == 0 { 1.5 } else if side == 1 { width() - 2.5 } else { 2 + rand(width() - 4) };
    let y = if side == 2 { 1.5 } else if side == 3 { height() - 2.5 } else { 2 + rand(height() - 4) };
    let kind = if wave >= 3 && i % 4 == 3 { "brute" } else if wave >= 2 && i % 5 == 4 { "sniper" } else { "drone" };
    create_prefab(kind, x, y);
    particles("sparkle", x + 0.5, y + 0.5);
}

fn on_ui(name) {
    if name == "start" {
        ui_remove("start");
        hide_screen();
        set_state("started", true);
        set_state("paused", false);
        set_game("score", 0);
        fade_in(30);
        sfx("blip");
        ui("hp_label", #{ type: "text", text: "HULL", x: 2, y: 2, size: 2.6, font: "mono", color: "#9fb8ff" });
        ui("hp", #{ type: "bar", x: 2, y: 5.5, w: 20, h: 2.6, value: 5, max: 5 });
        ui("score", #{ type: "text", text: "SCORE 0", x: 98, y: 2, anchor: "topright", size: 4, font: "mono" });
        ui("wave", #{ type: "text", text: "WAVE 0", x: 50, y: 2, anchor: "top", size: 3.5, font: "mono", color: "#7ee8ff" });
    }
    if name == "restart" { goto_level(level()); }
}
"""})

# --- prefabs ---
c({"cmd": "prefab", "name": "player", "props": {"kind": "ship", "physics": True, "w": 0.8, "h": 0.8, "sprite": "ship", "z": 5,
    "behaviors": ["topdown_player", "health", "gunner"], "hp": 5, "max_hp": 5, "invuln_ticks": 60, "speed": 7, "tags": ["player"]}})
c({"cmd": "prefab", "name": "shot", "props": {"kind": "shot", "physics": True, "gravity": 0, "w": 0.25, "h": 0.25, "die_on_wall": True,
    "lifetime": 50, "script": "projectile", "hits": "enemy", "damage": 1, "sprite": "shot", "z": 4}})
c({"cmd": "prefab", "name": "bullet", "props": {"kind": "ebullet", "physics": True, "gravity": 0, "w": 0.3, "h": 0.3, "die_on_wall": True,
    "lifetime": 150, "script": "projectile", "hits": "player", "damage": 1, "sprite": "ebullet", "z": 4}})
c({"cmd": "prefab", "name": "drone", "props": {"kind": "drone", "physics": True, "w": 0.7, "h": 0.7, "sprite": "drone", "z": 3,
    "behaviors": ["chaser", "enemy"], "hp": 1, "speed": 3.4, "sight": 60, "points": 10, "tags": ["enemy"]}})
c({"cmd": "prefab", "name": "brute", "props": {"kind": "brute", "physics": True, "w": 1.2, "h": 1.2, "sprite": "brute", "z": 3,
    "behaviors": ["chaser", "enemy"], "hp": 6, "speed": 1.7, "sight": 60, "points": 50, "tags": ["enemy"]}})
c({"cmd": "prefab", "name": "sniper", "props": {"kind": "sniper", "physics": True, "w": 0.8, "h": 0.8, "sprite": "sniper", "z": 3,
    "behaviors": ["chaser", "shooter", "enemy"], "hp": 2, "speed": 1.3, "sight": 60, "rate": 75, "range": 16, "bullet_speed": 9,
    "points": 25, "tags": ["enemy"]}})

c({"cmd": "create", "prefab": "player", "x": 15.6, "y": 9.6})
c({"cmd": "step", "ticks": 1})
c({"cmd": "save", "path": "worlds/neon-swarm"})

for cmd in cmds:
    print(json.dumps(cmd))
