// Behavior: topdown_player
// A top-down player that moves in 8 directions (Zelda style).
// Use with physics: true in a world without gravity ({"cmd":"physics","preset":"topdown"}).
// Props: speed (6 tiles/s), accel (60)
// Keys: arrows or WASD.

fn tick(me) {
    if state("paused", false) { set_vel(me, 0.0, 0.0); return; }
    let dx = 0;
    let dy = 0;
    if key("left") || key("a") { dx -= 1; }
    if key("right") || key("d") { dx += 1; }
    if key("up") || key("w") { dy -= 1; }
    if key("down") || key("s") { dy += 1; }
    // Diagonals aren't faster than straight lines.
    let k = if dx != 0 && dy != 0 { 0.7071 } else { 1.0 };
    let speed = prop(me, "speed", 6.0);
    let step = prop(me, "accel", 60.0) * dt();
    let vx = approach(prop(me, "vx", 0.0), dx * speed * k, step);
    let vy = approach(prop(me, "vy", 0.0), dy * speed * k, step);
    set_vel(me, vx, vy);
    if dx != 0 || dy != 0 { set(me, "facing", [dx, dy]); }
}
