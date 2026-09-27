// The knight: run, jump (coyote time + jump buffer + short hops), climb ladders.
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
        emit("coin", #{ x: other.x, y: other.y });
    } else if other.kind == "slime" {
        // Landing on top stomps it; anything else hurts.
        if p.vy > 0 && p.y + p.h - 0.45 <= other.y {
            destroy(other.id);
            set_vel(me, p.vx, -11.0);
            camera_shake(0.15);
            emit("stomp", #{ x: other.x, y: other.y });
        } else {
            hurt(me);
        }
    } else if other.kind == "checkpoint" {
        set_state("spawn", [other.x, other.y]);
        emit("checkpoint", #{ x: other.x, y: other.y });
    } else if other.kind == "flag" {
        set_state("won", true);
        emit("win", #{ x: other.x, y: other.y });
    }
}

fn hurt(me) {
    let lives = state("lives", 3) - 1;
    set_state("lives", lives);
    camera_shake(0.6);
    emit("hurt", #{ lives: lives });
    let sp = state("spawn", [2, 12]);
    set(me, "x", sp[0]);
    set(me, "y", sp[1]);
    set_vel(me, 0, 0);
    if lives <= 0 { set_state("over", true); }
}
