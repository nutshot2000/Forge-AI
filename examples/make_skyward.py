"""Builds worlds/skyward (a small platformer) entirely through forge commands.

    python examples/make_skyward.py > examples/skyward-build.jsonl
    target/release/forge.exe < examples/skyward-build.jsonl
"""
import json

W, H = 64, 16
g = [['.'] * W for _ in range(H)]


def fill(ch, x, y, w, h):
    for yy in range(y, y + h):
        for xx in range(x, x + w):
            if 0 <= xx < W and 0 <= yy < H:
                g[yy][xx] = ch


# ground with pits
fill('#', 0, 14, W, 2)
for px, pw in [(14, 3), (30, 4), (47, 3)]:
    fill('.', px, 14, pw, 2)
fill('#', 0, 0, 1, H)        # left wall
fill('#', W - 1, 0, 1, H)    # right wall
# hills and blocks
fill('#', 8, 12, 3, 2)
fill('#', 22, 11, 4, 3)
fill('#', 38, 12, 2, 2)
fill('#', 55, 10, 6, 4)
# one-way wooden platforms
fill('=', 12, 10, 4, 1)
fill('=', 18, 8, 3, 1)
fill('=', 27, 9, 3, 1)
fill('=', 41, 9, 4, 1)
# a ladder up to a high ledge with coins
fill('H', 35, 6, 1, 8)
fill('#', 32, 5, 7, 1)
# spikes
fill('^', 25, 10, 1, 1)
fill('^', 44, 13, 2, 1)
# grass on top of exposed ground
for y in range(H):
    for x in range(W):
        if g[y][x] == '#' and (y == 0 or g[y - 1][x] not in '#G') and 0 < x < W - 1:
            g[y][x] = 'G'
rows = [''.join(r) for r in g]

cmds = []
c = cmds.append
c({"cmd": "new", "map": rows, "seed": 3})
c({"cmd": "physics", "preset": "platformer"})

# --- art (8x8 pixel sprites, palettes as css colors) ---
c({"cmd": "sprite", "name": "knight", "palette": {"h": "#d7dee8", "v": "#1d2433", "b": "#4a78e0", "d": "#2b3f7a", "s": "#f5c542", "l": "#3a2a1a"},
   "pixels": ["..hhhh..", ".hhhhhh.", ".hvvvvh.", ".hhhhhh.", "sbbbbbb.", "s.bddb..", "..b..b..", "..l..l.."]})
KNIGHT_PAL = {"h": "#d7dee8", "v": "#1d2433", "b": "#4a78e0", "d": "#2b3f7a", "s": "#f5c542", "l": "#3a2a1a"}
c({"cmd": "sprite", "name": "knight_run", "palette": KNIGHT_PAL, "fps": 10,
   "pixels": ["..hhhh..", ".hhhhhh.", ".hvvvvh.", ".hhhhhh.", "sbbbbbb.", "s.bddb..", ".b....b.", ".l....l."],
   "frames": [["..hhhh..", ".hhhhhh.", ".hvvvvh.", ".hhhhhh.", "sbbbbbb.", "s.bddb..", "...bb...", "...ll..."]]})
c({"cmd": "sprite", "name": "knight_jump", "palette": KNIGHT_PAL,
   "pixels": ["..hhhh..", ".hhhhhh.", ".hvvvvh.", ".hhhhhh.", "sbbbbbbs", "..bddb..", ".bb..bb.", ".l....l."]})
c({"cmd": "sprite", "name": "slime", "palette": {"g": "#6fd35f", "d": "#3a9a3a", "w": "#ffffff", "k": "#10200f"},
   "pixels": ["........", "........", "..gggg..", ".gwggwg.", ".gkggkg.", "gggggggg", "gddddddg", ".gggggg."], "fps": 4,
   "frames": [["........", "........", "........", "..gggg..", ".gwggwg.", "ggkggkgg", "gddddddg", "gggggggg"]]})
c({"cmd": "sprite", "name": "coin", "palette": {"y": "#f5c542", "o": "#d99a1e", "w": "#fff4c2"},
   "pixels": ["..yyyy..", ".yowyyy.", "yoyyyyoy", "yoyyyyoy", "yoyyyyoy", "yoyyyyoy", ".yooooy.", "..yyyy.."], "fps": 8,
   "frames": [["...yy...", "..yowy..", "..yoyy..", "..yoyy..", "..yoyy..", "..yoyy..", "..yooy..", "...yy..."],
              ["...yy...", "...wy...", "...yy...", "...yy...", "...yy...", "...yy...", "...oy...", "...yy..."],
              ["...yy...", "..ywoy..", "..yyoy..", "..yyoy..", "..yyoy..", "..yyoy..", "..yooy..", "...yy..."]]})
c({"cmd": "sprite", "name": "flag", "palette": {"p": "#c9d1d9", "r": "#ff5f6d", "w": "#ffd0d4"},
   "pixels": ["prrrr...", "prrwrr..", "prrrrrr.", "prrrr...", "p.......", "p.......", "p.......", "p......."], "fps": 3,
   "frames": [["prrr....", "prrwrrr.", "prrrrr..", "prrrrr..", "p.......", "p.......", "p.......", "p......."]]})
c({"cmd": "sprite", "name": "checkpoint", "palette": {"p": "#c9d1d9", "b": "#5fd3c9"},
   "pixels": ["pbbb....", "pbbbb...", "pbbb....", "p.......", "p.......", "p.......", "p.......", "p......."]})
c({"cmd": "sprite", "name": "grass", "palette": {"g": "#4fae3a", "l": "#78d45e", "d": "#6b4a2e", "e": "#5a3c24"},
   "pixels": ["llgllglg", "gggggggg", "dgdddgdd", "dddddddd", "ddeddddd", "dddddedd", "dedddddd", "dddddded"]})
c({"cmd": "sprite", "name": "dirt", "palette": {"d": "#6b4a2e", "e": "#5a3c24", "r": "#7a5638"},
   "pixels": ["dddddddd", "ddeddddd", "dddddrdd", "dddddddd", "drdddded", "dddddddd", "ddddeddd", "eddddddd"]})
c({"cmd": "sprite", "name": "plank", "palette": {"w": "#b07a45", "d": "#7a4f2a"},
   "pixels": ["wwwwwwww", "dwwwdwww", "dddddddd", "........", "........", "........", "........", "........"]})
c({"cmd": "sprite", "name": "ladder", "palette": {"w": "#b07a45"},
   "pixels": [".w....w.", ".wwwwww.", ".w....w.", ".w....w.", ".w....w.", ".wwwwww.", ".w....w.", ".w....w."]})
c({"cmd": "sprite", "name": "spikes", "palette": {"s": "#d7dee8", "d": "#8a93a6"},
   "pixels": ["........", "........", "...s...s", "..ss..ss", "..sd..sd", ".ssd.ssd", ".sdd.sdd", "sddssdds"]})
c({"cmd": "sprite", "name": "lift", "palette": {"m": "#9aa4b8", "d": "#5c6577", "y": "#f5c542"},
   "pixels": ["mmmmmmmm", "dydydydy", "dddddddd", "........", "........", "........", "........", "........"]})

# --- backdrop: sky gradient + parallax layers (pixel art generated below) ---
import math


def mountains(w=64, h=20):
    rows = []
    for y in range(h):
        row = ""
        for x in range(w):
            peak = 7 + 5 * abs(math.sin(x * 0.11)) + 4 * abs(math.sin(x * 0.043 + 1.3))
            top = h - peak
            row += "s" if top <= y < top + 1.2 and peak > 12 else ("m" if y >= top else ".")
        rows.append(row)
    return rows


def hills(w=48, h=10):
    rows = []
    for y in range(h):
        row = ""
        for x in range(w):
            top = h - (4 + 2.5 * math.sin(x * 0.26) + 1.5 * math.sin(x * 0.09 + 2))
            row += "l" if top <= y < top + 1 else ("h" if y >= top else ".")
        rows.append(row)
    return rows


def clouds(w=64, h=8):
    blobs = [(8, 4, 6, 2.2), (14, 3, 5, 2.5), (40, 5, 7, 2), (47, 4, 4, 2.2)]
    rows = []
    for y in range(h):
        row = ""
        for x in range(w):
            row += "c" if any(((x - bx) / rx) ** 2 + ((y - by) / ry) ** 2 <= 1 for bx, by, rx, ry in blobs) else "."
        rows.append(row)
    return rows


c({"cmd": "sprite", "name": "bg_mountains", "palette": {"m": "#2c3b63", "s": "#8ea3d1"}, "pixels": mountains()})
c({"cmd": "sprite", "name": "bg_hills", "palette": {"h": "#23462f", "l": "#2f5c3c"}, "pixels": hills()})
c({"cmd": "sprite", "name": "bg_clouds", "palette": {"c": "#3d5286"}, "pixels": clouds()})
c({"cmd": "background", "sky": ["#101a33", "#2a4a7a", "#5b7fb8"], "layers": [
    {"sprite": "bg_clouds", "parallax": 0.1, "y": 1, "height": 3},
    {"sprite": "bg_mountains", "parallax": 0.25, "y": 3.5, "height": 9},
    {"sprite": "bg_hills", "parallax": 0.5, "y": 9, "height": 6},
]})
for name, preset in [("jump", "jump"), ("coin", "coin"), ("stomp", "stomp"), ("hurt", "hit"), ("checkpoint", "powerup"), ("win", "win")]:
    c({"cmd": "sound", "name": name, "preset": preset})
c({"cmd": "tile", "char": "G", "name": "grass", "solid": True, "sprite": "grass"})
c({"cmd": "tile", "char": "#", "name": "dirt", "solid": True, "sprite": "dirt"})
c({"cmd": "tile", "char": "=", "name": "plank", "platform": True, "sprite": "plank"})
c({"cmd": "tile", "char": "H", "name": "ladder", "ladder": True, "sprite": "ladder"})
c({"cmd": "tile", "char": "^", "name": "spikes", "solid": False, "sprite": "spikes"})

# --- behaviour ---
c({"cmd": "script", "name": "player", "code": """// The knight: run, jump (coyote time + jump buffer + short hops), climb ladders.
fn tick(me) {
    let p = get(me);
    if state("over", false) || state("won", false) { set_vel(me, 0.0, p.vy); return; }
    let dir = 0;
    if key("left") || key("a") { dir -= 1; }
    if key("right") || key("d") { dir += 1; }

    // Run: quick to accelerate on the ground, a bit floatier in the air.
    let accel = if p.on_ground { 70.0 } else { 40.0 };
    let vx = approach(p.vx, dir * 7.5, accel * dt());
    let vy = p.vy;

    // Coyote time (jump just after leaving a ledge) and jump buffering (press slightly early).
    let jump_pressed = pressed("space") || pressed("z") || (pressed("up") && !p.on_ladder) || (pressed("w") && !p.on_ladder);
    let jump_held = key("space") || key("z") || key("up") || key("w");
    let coyote = if p.on_ground { 7 } else { max(0, p.coyote - 1) };
    let buffer = if jump_pressed { 8 } else { max(0, p.buffer - 1) };
    if buffer > 0 && coyote > 0 {
        jump(me, 3.3);
        vy = get(me).vy;
        coyote = 0;
        buffer = 0;
        sfx("jump");
        emit("jump");
    }
    // Let go early for a short hop.
    if vy < -5.0 && !jump_held { vy = -5.0; }

    // Ladders: hang on, climb up/down.
    if p.on_ladder {
        let climb = 0;
        if key("up") || key("w") { climb = -1; }
        if key("down") || key("s") { climb = 1; }
        if climb != 0 || !p.on_ground {
            set(me, "gravity", 0);
            vy = climb * 5.0;
        }
    } else if p.gravity != 1 {
        set(me, "gravity", 1);
    }

    set_vel(me, vx, vy);
    set(me, "coyote", coyote);
    set(me, "buffer", buffer);

    // Spikes and falling out of the world hurt.
    if overlaps_tile(me, "^") || p.y > height() { hurt(me); }
}

fn on_touch(me, other) {
    let p = get(me);
    if other.kind == "coin" {
        destroy(other.id);
        set_state("coins", state("coins", 0) + 1);
        sfx("coin");
        emit("coin", #{ x: other.x, y: other.y });
    } else if other.kind == "slime" {
        // Landing on top stomps it; anything else hurts.
        if p.vy > 0 && p.y + p.h - 0.45 <= other.y {
            destroy(other.id);
            set_vel(me, p.vx, -11.0);
            camera_shake(0.15);
            sfx("stomp");
            emit("stomp", #{ x: other.x, y: other.y });
        } else {
            hurt(me);
        }
    } else if other.kind == "checkpoint" {
        if state("spawn", [0, 0])[0] != other.x { sfx("checkpoint"); }
        set_state("spawn", [other.x, other.y]);
        emit("checkpoint", #{ x: other.x, y: other.y });
    } else if other.kind == "flag" {
        if !state("won", false) { sfx("win"); }
        set_state("won", true);
        emit("win", #{ x: other.x, y: other.y });
    }
}

fn hurt(me) {
    let lives = state("lives", 3) - 1;
    set_state("lives", lives);
    camera_shake(0.6);
    sfx("hurt");
    emit("hurt", #{ lives: lives });
    let sp = state("spawn", [2, 12]);
    set(me, "x", sp[0]);
    set(me, "y", sp[1]);
    set_vel(me, 0, 0);
    if lives <= 0 { set_state("over", true); }
}
"""})
c({"cmd": "script", "name": "slime", "code": """// Patrols back and forth; turns around at walls and at ledges.
fn tick(me) {
    let s = get(me);
    let dir = s.dir;
    if s.hit_wall != 0 {
        dir = -s.hit_wall;
    } else if s.on_ground {
        let ahead = if dir > 0 { s.x + s.w + 0.05 } else { s.x - 0.05 };
        let below = tile(ahead, s.y + s.h + 0.1);
        if !solid(ahead, s.y + s.h + 0.1) && below != "=" { dir = -dir; }
    }
    set(me, "dir", dir);
    set_vel(me, dir * s.speed, s.vy);
}
"""})
c({"cmd": "script", "name": "mover", "code": """// A moving platform: slides between x0 and x0 + range.
fn tick(me) {
    let m = get(me);
    if m.x >= m.x0 + m.range && m.vx > 0 { set(me, "vx", -m.speed); }
    if m.x <= m.x0 && m.vx < 0 { set(me, "vx", m.speed); }
}
"""})
c({"cmd": "script", "name": "rules", "code": """// HUD, winning and losing.
fn rules() {
    let hearts = "";
    for i in 0..state("lives", 3) { hearts += "♥"; }
    hud("a_lives", "Lives " + hearts);
    hud("b_coins", "Coins " + state("coins", 0) + " / " + state("total", 0));
    if state("won", false) { hud("c_msg", "You reached the flag! Coins: " + state("coins", 0)); }
    else if state("over", false) { hud("c_msg", "Game over. Press Undo or restart"); }
}
"""})

# --- prefabs ---
c({"cmd": "prefab", "name": "player", "props": {"kind": "player", "script": "player", "sprite": "knight", "physics": True, "w": 0.75, "h": 0.9,
                                                "flip": "auto", "z": 5, "coyote": 0, "buffer": 0, "gravity": 1, "tags": ["player"],
                                                "anims": {"idle": "knight", "run": "knight_run", "jump": "knight_jump"}}})
c({"cmd": "prefab", "name": "slime", "props": {"kind": "slime", "script": "slime", "sprite": "slime", "physics": True, "w": 0.9, "h": 0.7,
                                               "dir": -1, "speed": 2.0, "flip": "auto", "z": 3, "tags": ["enemy"]}})
c({"cmd": "prefab", "name": "coin", "props": {"kind": "coin", "sprite": "coin", "w": 0.6, "h": 0.6, "z": 2, "tags": ["pickup"]}})
c({"cmd": "prefab", "name": "flag", "props": {"kind": "flag", "sprite": "flag", "w": 1, "h": 2, "z": 1}})
c({"cmd": "prefab", "name": "checkpoint", "props": {"kind": "checkpoint", "sprite": "checkpoint", "w": 1, "h": 2, "z": 1}})
c({"cmd": "prefab", "name": "lift", "props": {"kind": "lift", "script": "mover", "sprite": "lift", "physics": "kinematic", "solid": "platform",
                                              "w": 3, "h": 1, "speed": 2.5, "vx": 2.5, "z": 1}})

# --- level ---
c({"cmd": "create", "prefab": "player", "x": 2, "y": 13.1})
for x, y in [(9, 12.3), (19, 11.3), (29, 12.3), (43, 13.3)]:
    c({"cmd": "create", "prefab": "slime", "x": x, "y": y})
coins = [(5, 12.5), (9.2, 10.8), (13.2, 8.6), (14.2, 8.6), (19.2, 6.6), (23.2, 9.6), (28.2, 7.6), (32.2, 3.6), (33.2, 3.6), (34.2, 3.6),
         (36.2, 3.6), (37.2, 3.6), (42.2, 7.6), (43.2, 7.6), (48.2, 10.6), (52.2, 12.5), (57.2, 8.6), (59.2, 8.6)]
for x, y in coins:
    c({"cmd": "create", "prefab": "coin", "x": x, "y": y})
c({"cmd": "create", "prefab": "lift", "x": 29.5, "y": 11, "props": {"x0": 29.5, "range": 4.5}})
c({"cmd": "create", "prefab": "checkpoint", "x": 27, "y": 12})
c({"cmd": "create", "prefab": "flag", "x": 61, "y": 8})
c({"cmd": "exec", "code": 'set_state("total", count("coin")); set_state("lives", 3); set_state("coins", 0); set_state("spawn", [2, 13.1]); hud("c_msg", "Arrows or WASD to move, Space to jump. Reach the flag!"); count("coin")'})
c({"cmd": "camera", "follow": 1, "view": [24, 14], "lerp": 0.15, "tile_size": 16})
c({"cmd": "step", "ticks": 1})
c({"cmd": "save", "path": "worlds/skyward"})

for cmd in cmds:
    print(json.dumps(cmd))
