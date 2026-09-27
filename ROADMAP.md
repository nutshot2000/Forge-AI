# forge roadmap

**Goal:** a proper 2D game engine where the AI is the user's right arm. You and the agent work on the same live
game: anything you can see or click, the agent can see and do in one command; each of you always knows what the
other changed; and anything either of you does can be undone.

☑ = done, ◐ = partly done, ✗ = dropped. Items have IDs (e.g. `P1.4`) so we can refer to them ("let's do P1.4 next"). ☐ = to do, ☑ = done.

---

## AI-first rules (every feature must follow these)

1. **Every button is a command.** If the editor can do it, the agent can do it through the same command, with no UI-only features.
2. **Everything is visible to the agent.** New state shows up in `look` (summarised) and has a command to read it in full.
3. **Text first.** Worlds, sprites, maps, sounds, music, UI and levels are stored as readable text the agent can write directly.
4. **Batchable and fast.** A whole edit → test → inspect loop fits in one tool call; responses are compact by default with detail on request.
5. **Attributed and undoable.** Every change is tagged 🤖 agent or 🧑 you, logged in the activity feed, and undoable.
6. **Deterministic and testable.** Same world + same inputs = same result, so the agent can playtest headlessly and trust the numbers.
7. **Errors are guidance.** Every error says what went wrong *and* what to do instead, and points at the exact line or entity.
8. **The user is never lost.** Plain-language UI, no terminal needed, and the agent explains what it did in the editor itself.

---

## Done so far

- ☑ Engine core: text worlds, Rhai scripts, deterministic ticks, BFS pathfinding, events, snapshots, diff, batch playtests (`trials`)
- ☑ Agent link: MCP server with one batched `forge` tool; `look` shows the user's selection, hover, changes and requests
- ☑ Editor: viewport, scene tree, inspector (entities *and* tiles), scripts, assets, requests, console, activity feed, Play/Step
- ☑ Shared UI (agent can select, open scripts, switch tools), `say` toasts, `point` markers, undo/redo for both sides
- ☑ Right-click menus, requests pinned on the map, tags, prefabs, pixel-art sprites, tile types, HUD, keyboard input
- ☑ Import (images → sprites, txt/csv/Tiled maps, scripts, worlds) and export (world, map, sprites, scripts, screenshot)
- ☑ Forge desktop app with self-updating install; Coin Dash demo built entirely through commands

---

## Phase 0: Foundations (small, do first)

So we can move fast later without breaking things.

- ☑ **P0.1 Version history (git)** for the project, with a commit after each feature. Easy rollback if anything breaks.
- ☑ **P0.2 Autosave and crash recovery:** the open world autosaves every minute to `.autosave/`; the app offers to restore after a crash.
- ☑ **P0.3 Command test suite:** replay `.jsonl` sessions and compare results, so every command is covered and regressions get caught. (`cargo test`; CI workflow parked in `ci/` until the GitHub token has the `workflow` scope)
- ☑ **P0.4 Typed command schemas:** each command gets a JSON schema, so the agent gets exact argument errors and the MCP tool is more reliable.
- ☑ **P0.5 Transactions:** `{"cmd":"batch","atomic":true,...}` applies all or nothing, with one undo step for the whole batch.
- ☑ **P0.6 Dry run / preview:** any change command with `"preview":true` returns the diff without applying it; the editor can show "the agent wants to change X, Y, Z [Apply]".
- ☑ **P0.7 World browser in the app:** open, create, duplicate and rename worlds from a start screen, with no terminal.

---

## Phase 1: A real 2D engine (the big upgrade)

From grid-only to proper 2D, keeping grid mode for grid games.

- ☑ **P1.1 Free-movement mode:** entities get pixel positions (`px`, `py`), size/hitbox, velocity (`vx`, `vy`); grid mode stays the default for grid games.
- ☑ **P1.2 Fixed 60 Hz simulation** with smooth rendering (interpolation), separate from editor refresh.
- ☑ **P1.3 Tile collision:** entities slide along walls and never pass through; `on_ground`, `hit_wall` flags; one-way platforms; ladders.
- ☑ **P1.4 Gravity and physics presets:** gravity, friction, bounce, max speed as world or entity settings; `platformer` and `topdown` presets.
- ☑ **P1.5 Entity collisions and triggers:** `fn on_touch(me, other)` hooks, trigger zones (areas that fire when entered), collision layers/masks.
- ☑ **P1.6 Transform:** rotation, scale, flip, opacity, tint per entity.
- ☑ **P1.7 Camera:** follow a target with smoothing, dead zone, map bounds, zoom, screen shake; `camera` command for the agent.
- ◐ **P1.8 Big worlds:** maps of any size, stored in chunks, only nearby parts simulated/drawn; minimap. (Done: camera, only visible tiles drawn, editor zoom/pan, minimap. To do: chunked storage for huge maps.)
- ◐ **P1.9 Layers:** background, parallax layers, tiles, decorations, entities, foreground, UI; hide/lock per layer. (Done: sky gradient + parallax backdrop layers via `background`, entity `z` order. To do: decoration/foreground tile layers, hide/lock.)
- ☑ **P1.10 Raycasts and line of sight:** `raycast(x1,y1,x2,y2)`, `can_see(a,b)` for enemies, lasers and AI vision.
- ☑ **P1.11 Pathfinding for free movement:** navigation over the tile grid, then smooth steering to follow the path.
- ☑ **Milestone demo:** Skyward (`worlds/skyward`, built by `examples/make_skyward.py` through engine commands): run, jump with coyote time and buffering, stomp slimes, coins, one-way planks, a ladder, spikes, pits, a moving platform, checkpoint, flag, following camera.

## Phase 2: Runs anywhere (game runtime in the browser)

- ☑ **P2.1 Compile the engine to WebAssembly** so the exact same simulation runs inside the editor window.
- ☑ **P2.2 Smooth local Play mode:** Play runs in the window at 60 fps with instant input; the engine stays in charge of edits and the agent sees the live state.
- ☑ **P2.3 Export as one playable `.html` file:** double-click to play, share with anyone.
- ☐ **P2.4 Publish to the web:** one click (or one agent command) publishes the game as a shareable page; itch.io-ready zip.
- ☐ **P2.5 Controls:** gamepad support, on-screen touch controls for phones, key rebinding.
- ☐ **P2.6 Export as a desktop `.exe`** (a stretch goal).
- ◐ **Milestone:** Coin Dash and the platformer demo exported and playable on a phone. (Export works and includes touch buttons; not yet tried on a real phone.)

## Phase 3: Art, animation and sound (all agent-writable text)

- ☑ **P3.1 Animation:** sprites with multiple frames and fps; named animations (`idle`, `walk`, `jump`, `hurt`); auto-switch from movement state.
- ☐ **P3.2 Pixel editor** in the editor: pencil, fill, mirror, palette, frames timeline, onion skin, plus "ask the agent to draw/edit this".
- ☐ **P3.3 Sprite sheets:** import a sheet image and slice it into frames; export sheets.
- ☐ **P3.4 Tilesets and autotiling:** walls pick the right edge/corner piece automatically; 16×16/32×32 tile sizes.
- ☐ **P3.5 Particles:** dust, sparks, explosions, rain, defined as short text presets (`particles("explosion", x, y)`).
- ◐ **P3.6 Sound effects:** synthesized from parameters (jsfxr-style), so the agent can design sounds as text; `sfx("jump")`; import `.wav`/`.ogg`. (Done: `sound` recipes + 9 presets, `sfx()`, mute button, plays in exported games. To do: importing audio files.)
- ☐ **P3.7 Music:** a tiny text tracker format the agent can compose in; loops, per-level music, fade.
- ☐ **P3.8 Text and fonts:** pixel fonts, in-world text, floating damage numbers.
- ☐ **P3.9 Lighting and effects (stretch):** darkness, light radius, simple shaders (flash, outline, palette swap).
- ☐ **P3.10 Palettes and style lock:** a project palette/style guide the agent follows so all art matches.

## Phase 4: Structure for full games

- ☑ **P4.1 Scenes and levels:** multiple maps per project, `goto("level2")`, doors/portals, level list and ordering in the editor.
- ◐ **P4.2 Game flow:** title screen, pause menu, game over, win screen, credits, built from simple text UI definitions. (Done: show_screen overlays, levels via goto_level. To do: menus with choices.)
- ☐ **P4.3 UI system:** buttons, bars, text boxes, inventory grids, defined in text, laid out automatically, clickable in-game.
- ☐ **P4.4 Dialogue:** conversation trees with choices and conditions, in a readable text format.
- ◐ **P4.5 Game state and saves:** variables that persist across levels; save/load slots. (Done: game()/set_game() values that carry across levels. To do: save slots.)
- ☑ **P4.6 Timers, tweens and coroutines:** `after(30, || ...)`, `every(60, ...)`, smooth movement tweens, wait-until.
- ☑ **P4.7 Messages between entities:** `send(target, "open")` with `fn on_message(me, msg)`; global events.
- ☐ **P4.8 State machines:** `idle → chase → attack` states declared in data, visualised in the editor.
- ☑ **P4.9 Behaviour library:** ready-made scripts (platformer player, top-down player, patrol, chase, shooter, spawner, pickup, door+key, health) that can be mixed.
- ☐ **P4.10 Spawners and waves:** timed/random spawning with difficulty ramps.
- ☐ **Milestone:** a complete small game with a title screen, 3 levels, dialogue, saving and music.

## Phase 5: Scripting that is pleasant for both of us

- ☐ **P5.1 Nicer script API:** `me.x += 1`, `me.hp`, `other.kind` instead of `get`/`set` calls (both keep working).
- ☐ **P5.2 Errors in place:** script errors highlight the exact line in the editor and the entity that hit them; one-click "ask the agent to fix".
- ☐ **P5.3 Script editor upgrade:** syntax highlighting, autocomplete for the engine API, hover docs, go-to-definition.
- ☐ **P5.4 Hot reload keeping state:** editing a script while playing doesn't reset the game.
- ☐ **P5.5 Debugger:** breakpoints, step, watch values, "pause when #3's hp < 2".
- ☐ **P5.6 Profiler:** time per script and entity; the agent gets "slime script uses 70% of frame time".
- ☐ **P5.7 Shared script modules:** `import "utils"` for common functions.

## Phase 6: The right arm (AI-first superpowers)

The part that makes forge different from other engines.

**Seeing**
- ☑ **P6.1 Screenshots for the agent:** a `screenshot` command returns a rendered image through MCP, so the agent can *see* the game (art quality, layout) as well as read it.
- ☐ **P6.2 Look v2:** a plain-language scene summary ("hero is boxed in by water on the left; 3 slimes guard the east room"), a region view for huge maps, and what's on screen right now.
- ☐ **P6.3 Watch mode:** the agent can subscribe to events ("tell me when the player dies") and get a summary of what happened since last time.

**Acting**
- ✗ **P6.4 In-editor agent:** dropped. You drive Forge from your own agent (Claude Code, Cursor, …) through MCP, which works better in practice than a chat box inside the app.
- ☐ **P6.5 Agent checkpoints:** the agent auto-snapshots before any big change; "⟲ undo everything the agent did in that request" in one click.
- ☐ **P6.6 Suggestion cards:** the agent can propose changes as cards (preview diff + Apply/Dismiss) instead of applying them directly, when you want to stay in control.
- ☐ **P6.7 Parallel agents:** several agents work on copies (one does art, one levels, one balancing) and merge back with a visual diff.

**Testing and balancing**
- ☐ **P6.8 Play bots:** built-in bot players (random, explorer, goal-seeker, "speedrunner") the agent points at a level to test it.
- ☐ **P6.9 Level checks:** reachability (can every coin/exit be reached?), soft-locks, stuck entities, entities inside walls.
- ☐ **P6.10 Balancing sweeps:** try a range of values (enemy speed 1–5 × damage 1–3) over many runs and report the best settings; charts in the editor.
- ☐ **P6.11 Game health panel:** a continuous validator (script errors, missing sprites, unused assets, unreachable areas, performance) as clickable issues with "ask the agent to fix".
- ☐ **P6.12 Replays:** record a play session (yours or a bot's) and replay it exactly; attach replays to bug requests ("this happened").

**Knowing the project**
- ☐ **P6.13 Game design doc:** a `GAME.md` per project that the agent keeps up to date (idea, rules, controls, style, todo) and reads first, so it remembers intent across sessions.
- ☐ **P6.14 Explain mode:** hover anything to get a plain-language "what this does and why" from its script and tags.
- ☐ **P6.15 Recipe library:** reusable how-tos ("add a double jump", "make a boss with 3 phases") the agent can apply and adapt, growing as we build.
- ☐ **P6.16 Timeline scrubber:** drag back through recorded ticks to see exactly what happened; the agent can reference "tick 212" and you jump there.

**Creating**
- ☐ **P6.17 Generators:** procedural level/dungeon/terrain generators as scripts the agent writes and tunes ("make 5 cave levels, each harder").
- ☐ **P6.18 Style-consistent art generation:** the agent draws sprite sets that follow the project palette and size (P3.10).
- ☐ **P6.19 Templates:** start a new game from platformer / top-down shooter / roguelike / puzzle / snake / tower defense templates, then customise with the agent.

## Phase 7: Editor polish

- ☐ **P7.1 Multi-select:** box select, shift-click, move/copy/delete groups, align and distribute.
- ☐ **P7.2 Tile brushes:** rectangle, fill, line, circle, random scatter, stamp (copy a region and paste it).
- ☐ **P7.3 Zoom and pan** in the viewport, grid toggle, rulers, coordinates readout.
- ☐ **P7.4 Layers panel** (with P1.9), show/hide, lock.
- ☐ **P7.5 Properties with the right controls:** colour pickers, sliders for numbers, dropdowns for known values, checkboxes for true/false.
- ☐ **P7.6 Onboarding:** a first-run tour, a shortcuts cheat sheet (press `?`), and example projects.
- ☐ **P7.7 Settings:** theme (light/dark), UI scale, autosave interval, default tile size.
- ☐ **P7.8 Recording:** capture a GIF/video of gameplay to share.
- ☐ **P7.9 Resizable panels** and layout presets (focus mode: big viewport).

## Phase 8: Quality and scale

- ☐ **P8.1 Performance:** 10,000 entities at 60 fps; spatial index; cached pathfinding; profiling in CI.
- ☐ **P8.2 Save format versioning** with automatic upgrades of old worlds.
- ☐ **P8.3 Git-friendly world files:** split big worlds into several files so changes diff cleanly.
- ☐ **P8.4 Security:** scripts are sandboxed (already capped); limits on file access for imports/exports.
- ☐ **P8.5 Docs site** generated from the command/script reference, with examples.

---

## Suggested order

1. **Phase 0** (P0.1–P0.3 first: git, autosave, tests)
2. **Phase 1 + P2.1–P2.3** together: real 2D + smooth play + export, then the platformer milestone
3. **Phase 4 core** (timers, messages, levels, game screens, behaviour library), then **P6.1, P6.9, P6.11**: agent screenshots, level checks, game health
4. **Phase 3**: animation, sound, pixel editor
5. **Phase 4**: levels, menus, dialogue, then the "complete game" milestone
6. Then Phases 5, 6 (the rest), 7 and 8, interleaved based on what the games we make need
