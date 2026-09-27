// Chase the hero when it's in sight, otherwise wander. Bites on contact, then cools down.
fn tick(me) {
    let s = get(me);
    let heroes = find("hero");
    if heroes.is_empty() { return; }
    let h = heroes[0];
    if s.cooldown > 0 { set(me, "cooldown", s.cooldown - 1); }
    let dist = abs(h.x - s.x) + abs(h.y - s.y);
    if dist <= 1 && s.cooldown == 0 && h.hp > 0 {
        set(h.id, "hp", h.hp - s.dmg);
        set(me, "cooldown", 4);
        emit("hit", #{ x: h.x, y: h.y, hp: h.hp - s.dmg });
        return;
    }
    // Slimes are slow: they only move every other tick.
    if now() % 2 == 0 { return; }
    if dist <= s.sight {
        move_toward(me, h.x, h.y);
    } else {
        let d = [[1, 0], [-1, 0], [0, 1], [0, -1]][rand(4)];
        move_by(me, d[0], d[1]);
    }
}
