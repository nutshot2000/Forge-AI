// Walk (with pathfinding) to the nearest coin and pick it up.
fn tick(me) {
    let h = get(me);
    let coins = find("coin");
    if coins.is_empty() { return; }

    let best = coins[0];
    let bd = 1000000;
    for c in coins {
        let d = abs(c.x - h.x) + abs(c.y - h.y);
        if d < bd { bd = d; best = c; }
    }
    move_toward(me, best.x, best.y);

    let h = get(me);
    for e in at(h.x, h.y) {
        if e.kind == "coin" {
            destroy(e.id);
            set(me, "gold", h.gold + 1);
            emit("coin_picked", #{ x: h.x, y: h.y, gold: h.gold + 1 });
        }
    }
}
