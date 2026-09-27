// Bite a hero standing next to us, otherwise wander randomly.
fn tick(me) {
    let s = get(me);
    for h in near(s.x, s.y, 1) {
        if h.kind == "hero" {
            let hp = h.hp - s.dmg;
            set(h.id, "hp", hp);
            emit("hit", #{ by: me, hp: hp });
            if hp <= 0 {
                emit("death", #{ id: h.id, killer: me, gold: h.gold });
                destroy(h.id);
            }
            return;
        }
    }
    let dirs = [[1, 0], [-1, 0], [0, 1], [0, -1]];
    let d = dirs[rand(4)];
    move_by(me, d[0], d[1]);
}
