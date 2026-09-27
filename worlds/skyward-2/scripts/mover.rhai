// A moving platform: slides between x0 and x0 + range.
fn tick(me) {
    let m = get(me);
    if m.x >= m.x0 + m.range && m.vx > 0 { set(me, "vx", -m.speed); }
    if m.x <= m.x0 && m.vx < 0 { set(me, "vx", m.speed); }
}
