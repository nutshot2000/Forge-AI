// Behavior: chaser
// Hunts the nearest entity tagged "player" within sight, finding its way around walls.
// Physics bodies steer smoothly along the path (flying enemies, top-down monsters);
// grid entities step one tile every few ticks.
// Props: sight (8 tiles), speed (3 tiles/s, physics), step_every (4 ticks, grid), target_tag ("player")

fn tick(me) {
    let targets = tagged(prop(me, "target_tag", "player"));
    let is_body = prop(me, "physics", false) == true;
    if targets.is_empty() {
        if is_body { set_vel(me, 0.0, 0.0); }
        return;
    }
    let target = targets[0];
    let best = 1000000.0;
    for t in targets {
        let d = dist(me, t.id);
        if d < best { best = d; target = t; }
    }
    if best > prop(me, "sight", 8.0) {
        if is_body { set_vel(me, 0.0, 0.0); }
        return;
    }
    let c = center(target.id);
    if is_body {
        let d = path_dir(me, c[0], c[1]);
        let speed = prop(me, "speed", 3.0);
        set_vel(me, d[0] * speed, d[1] * speed);
    } else if now() % prop(me, "step_every", 4) == 0 {
        move_toward(me, c[0], c[1]);
    }
}
