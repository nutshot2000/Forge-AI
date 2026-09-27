// Arrow keys or WASD move the hero one tile per tick.
fn tick(me) {
    let p = get(me);
    if p.hp <= 0 { return; }
    let dx = 0;
    let dy = 0;
    if key("left") || key("a") { dx = -1; }
    else if key("right") || key("d") { dx = 1; }
    else if key("up") || key("w") { dy = -1; }
    else if key("down") || key("s") { dy = 1; }
    if dx != 0 || dy != 0 { move_by(me, dx, dy); }
    let p = get(me);
    for e in at(p.x, p.y) {
        if e.kind == "coin" {
            destroy(e.id);
            set(me, "gold", p.gold + 1);
            emit("coin_picked", #{ x: p.x, y: p.y });
        }
    }
}
