// Behavior: platformer_player
// A side-view player: run, jump with coyote time and jump buffering, short hops, ladders.
// Use with physics: true in a world with gravity ({"cmd":"physics","preset":"platformer"}).
// Props: speed (7.5 tiles/s), jump_height (3.3 tiles), accel (70), air_accel (40), climb_speed (5)
// Keys: left/right or A/D to run; Space, Z, Up or W to jump; Up/Down on ladders.
// Emits "jump" and plays sfx("jump").

fn tick(me) {
    if state("paused", false) { return; }
    let dir = 0;
    if key("left") || key("a") { dir -= 1; }
    if key("right") || key("d") { dir += 1; }
    let on_ground = prop(me, "on_ground", false);
    let on_ladder = prop(me, "on_ladder", false);

    let accel = if on_ground { prop(me, "accel", 70.0) } else { prop(me, "air_accel", 40.0) };
    let vx = approach(prop(me, "vx", 0.0), dir * prop(me, "speed", 7.5), accel * dt());
    let vy = prop(me, "vy", 0.0);

    // Coyote time: jump just after leaving a ledge. Buffer: press jump slightly early.
    let jump_pressed = pressed("space") || pressed("z") || ((pressed("up") || pressed("w")) && !on_ladder);
    let jump_held = key("space") || key("z") || key("up") || key("w");
    let coyote = if on_ground { 7 } else { max(0, prop(me, "coyote", 0) - 1) };
    let buffer = if jump_pressed { 8 } else { max(0, prop(me, "buffer", 0) - 1) };
    if buffer > 0 && coyote > 0 {
        jump(me, prop(me, "jump_height", 3.3));
        vy = get(me).vy;
        coyote = 0;
        buffer = 0;
        sfx("jump");
        emit("jump", #{ id: me });
    }
    // Let go early for a short hop.
    if vy < -5.0 && !jump_held { vy = -5.0; }

    // Ladders: hang on and climb.
    if on_ladder {
        let climb = 0;
        if key("up") || key("w") { climb = -1; }
        if key("down") || key("s") { climb = 1; }
        if climb != 0 || !on_ground {
            set(me, "gravity", 0);
            vy = climb * prop(me, "climb_speed", 5.0);
        }
    } else if prop(me, "gravity", 1) != 1 {
        set(me, "gravity", 1);
    }

    set_vel(me, vx, vy);
    set(me, "coyote", coyote);
    set(me, "buffer", buffer);
}
