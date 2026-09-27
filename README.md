# forge

A 2D game engine designed for AI agents first and humans second.

- **Worlds are text.** `world.json` (map + entities) and `scripts/*.rhai` (behaviors). Grep it, diff it, commit it.
- **One JSON line in, one JSON line out.** No editor, no screenshots, no per-click tool calls.
- **Agents write code, not clicks.** Behaviors are [Rhai](https://rhai.rs) scripts running inside the engine loop.
- **Deterministic and headless.** The RNG state is part of the world, so the same world always plays out the same way.
- **Built for experimenting.** Snapshot, rewind, diff and batch playtests are built in.

```
cargo build --release
forge worlds/dungeon < examples/tour.jsonl
```

## The editor

**Easiest:** open **Forge** from the Desktop or Start menu. That runs `forge-app.exe`, which starts the engine hidden
and opens the editor in its own window (Edge app mode). Closing the window stops the engine. If an agent's forge is
already running, the window shows that world instead.

From a terminal:

```
forge worlds/dungeon --editor      # then open http://127.0.0.1:7878
```

A browser-based editor for the live world: a viewport (click to select, drag to move, paint walls and floor,
place and erase entities), a scene list, an inspector for editing properties, a script editor with compile
errors, a console, Play/Pause/Step, snapshots and save. The activity feed tags every change as 🤖 agent, 🧑 you
or ⌨ cli. Simulations that run faster than real time are replayed so you can watch them.

When forge runs as an MCP server, the editor is always on. Open it to watch an agent work on the same world.
If port 7878 is taken it tries the next ones (the `state` command reports the URL). `--port N` picks a port and
`--no-editor` turns it off.

## 2D physics, camera and export

Units are tiles and seconds; an entity's `x`, `y` is the top-left of its `w` × `h` box. Grid games use whole numbers
and never see a fraction.

- `{"cmd":"physics","preset":"platformer"}` (gravity 60, 60 ticks/s), `"topdown"` or `"grid"`.
- Entity props: `physics: true` (dynamic body) or `"kinematic"`, `vx`/`vy`, `w`/`h`, `gravity`, `drag`, `bounce`,
  `max_speed`, `solid: true` or `"platform"` (moving platforms carry riders), `flip: "auto"`. The engine writes
  `on_ground`, `hit_wall`, `hit_ceiling`, `on_ladder`, `ground_id`.
- Tile types can be `solid`, one-way `platform` or `ladder`. Below the map is open, so things can fall into pits.
- Scripts: `fn on_touch(me, other)`, `jump(me, tiles)`, `set_vel`, `raycast`, `can_see`, `camera_shake`… (see `help`).
- `{"cmd":"camera","follow":1,"view":[24,14]}`; the editor's 🎥 Game view shows what the player sees.
- `{"cmd":"step","ticks":120,"inputs":{"0":["right"],"18":["right","space"]}}` plays with scripted keys.
- `{"cmd":"export_game","title":"My Game"}` writes `exports/My-Game.html`: one file with the engine (WebAssembly),
  renderer and world inside. Double-click to play; it has touch buttons on phones.

Demos: `worlds/coin-dash` (grid) and `worlds/skyward` (platformer, built by `examples/make_skyward.py`).

## As an MCP tool

`forge --mcp [world_dir]` serves the same protocol over MCP as one batched `forge` tool:
`{"commands": [{"cmd":"load",...}, {"cmd":"step",...}, {"cmd":"view"}]}`. Each call can run a whole
edit → simulate → inspect loop. It stops at the first failed command unless you pass `"keep_going": true`.

```
claude mcp add --scope user forge -- <forge folder>\app\forge.exe --mcp <forge folder>\worlds\coin-dash
```

The shortcut and the MCP registration run the installed copy in `app\`. After changing the engine, run `install.ps1`
(with Forge closed) to rebuild and update it, then start a new Claude session. `target\` is free to rebuild at any time.

See [ROADMAP.md](ROADMAP.md) for what's built and what's next.

## Development

- `cargo test --release` replays every session in `tests/sessions/*.jsonl` (each line is a command plus the result it should give).
- `node tests/wasm_smoke.mjs` checks the WebAssembly engine matches the native one.
- `install.ps1` builds and updates the installed app in `app\`; it works even while Forge is open (the next launch picks up the new version).
- Every command and its arguments are defined once in `src/commands.rs`; validation, `help` and the MCP tool schema are generated from it.

## Commands

| cmd | what it does |
|---|---|
| `state` | tick, size, entity counts by kind, scripts, vars, snapshots, compile errors |
| `view` | ASCII render (+ legend) of the whole map or a window |
| `query` | filter entities by `kind`, `near: [x,y,r]`, and a `where` expression like `e.hp < 5` |
| `get` | one entity |
| `exec` | run Rhai against the live world; variables persist between calls |
| `script` | read / list / replace a behavior script (compile-checked before it's installed) |
| `step` | run N ticks, optionally `until` an expression is true; returns event counts + recent events |
| `snapshot` / `restore` | save and rewind the full world state in memory |
| `diff` | what changed between a snapshot and now (entities, fields, tiles, vars, scripts) |
| `trials` | replay from a snapshot N times with different seeds and score each run with a `metric` |
| `events` | read the event log |
| `load` / `save` | world folders on disk |
| `help` | the full command + script API reference, as JSON |

## Script API

Entity scripts define `fn tick(me)`. A script named `rules` can define `fn rules()`, which runs once per tick after all entities.

```
get(id) set(id,key,val) create(kind,x,y[,props]) destroy(id)
find(kind) entities() at(x,y) near(x,y,r) count(kind)
move_by(id,dx,dy) move_toward(id,x,y)   # BFS pathfinding
path_len(x1,y1,x2,y2) tile(x,y) set_tile(x,y,ch) walkable(x,y)
now() rand(n) emit(kind[,data]) state(name[,default]) set_state(name,val) print(x)
```

Scripts are capped at 5M operations per call, so an infinite loop becomes an error message instead of a hang.
