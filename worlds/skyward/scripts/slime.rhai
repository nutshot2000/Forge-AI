// Patrols back and forth; turns around at walls and at ledges.
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
