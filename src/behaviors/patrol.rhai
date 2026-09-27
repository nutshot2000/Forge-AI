// Behavior: patrol
// Walks back and forth, turning around at walls and at ledges (so it never falls off).
// Works for platformer enemies (physics: true with gravity) and top-down guards.
// Props: speed (2 tiles/s), dir (-1 = start left, 1 = right)

fn tick(me) {
    let dir = prop(me, "dir", -1);
    let hit = prop(me, "hit_wall", 0);
    if hit != 0 {
        dir = -hit;
    } else if prop(me, "on_ground", false) {
        let e = get(me);
        let w = prop(me, "w", 1.0);
        let h = prop(me, "h", 1.0);
        let ahead = if dir > 0 { e.x + w + 0.05 } else { e.x - 0.05 };
        let below_y = e.y + h + 0.1;
        // Turn at ledges: nothing solid (and no one-way platform) ahead-and-below.
        if !solid(ahead, below_y) && tile(ahead, below_y) != "=" { dir = -dir; }
    }
    set(me, "dir", dir);
    set_vel(me, dir * prop(me, "speed", 2.0), prop(me, "vy", 0.0));
}
