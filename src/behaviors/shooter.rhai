// Behavior: shooter
// Fires at the nearest entity tagged "player" when it can see it and it's in range.
// Uses the prefab named by `bullet` if it exists, else makes a small glowing bullet.
// Props: rate (90 ticks between shots), range (10 tiles), bullet ("bullet"), bullet_speed (8), damage (1), target_tag ("player")
// Requires: projectile

fn tick(me) {
    if now() % prop(me, "rate", 90) != 0 { return; }
    let targets = tagged(prop(me, "target_tag", "player"));
    if targets.is_empty() { return; }
    let target = targets[0];
    if dist(me, target.id) > prop(me, "range", 10.0) || !can_see(me, target.id) { return; }
    let a = center(me);
    let b = center(target.id);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.01 { return; }
    let speed = prop(me, "bullet_speed", 8.0);
    let name = prop(me, "bullet", "bullet");
    let id = if has_prefab(name) {
        create_prefab(name, a[0] - 0.15, a[1] - 0.15, #{ from: me, hits: prop(me, "target_tag", "player") })
    } else {
        create("bullet", a[0] - 0.15, a[1] - 0.15, #{
            physics: true, gravity: 0, w: 0.3, h: 0.3, glyph: "*", color: "#ffd060", z: 4,
            script: "projectile", damage: prop(me, "damage", 1), from: me, hits: prop(me, "target_tag", "player")
        })
    };
    set_vel(id, dx / len * speed, dy / len * speed);
    sfx("laser");
    emit("shot", #{ from: me });
}
