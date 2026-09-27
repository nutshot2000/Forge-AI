// Behavior: moving_platform
// Slides back and forth and carries whatever stands on it. Set physics: "kinematic" and
// solid: "platform" (one-way) or solid: true.
// Props: axis ("x" or "y"), range (4 tiles), speed (2 tiles/s), start (its first position, set automatically)

fn tick(me) {
    let e = get(me);
    let axis = prop(me, "axis", "x");
    let pos = if axis == "y" { e.y } else { e.x };
    let start = prop(me, "start", ());
    if start == () {
        start = pos;
        set(me, "start", pos);
    }
    let speed = prop(me, "speed", 2.0);
    let v = prop(me, if axis == "y" { "vy" } else { "vx" }, speed);
    if v == 0 { v = speed; }
    if pos >= start + prop(me, "range", 4.0) && v > 0 { v = -speed; }
    if pos <= start && v < 0 { v = speed; }
    if axis == "y" { set_vel(me, 0.0, v); } else { set_vel(me, v, 0.0); }
}
