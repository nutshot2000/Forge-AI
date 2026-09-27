// Behavior: projectile
// A bullet: flies until it hits a wall or its lifetime runs out (the engine handles both, so
// hundreds are cheap), and damages what it's aimed at. On touching an entity tagged `hits`, sends
// it a "damage" message (see the health behavior) and disappears with a small burst.
// Use with physics: true, gravity: 0 (or > 0 for arcs), die_on_wall: true, lifetime (ticks).
// Props: damage (1), hits ("player"), from (who fired it; never hits them)

fn on_touch(me, other) {
    if other.id == prop(me, "from", -1) { return; }
    if has_tag(other.id, prop(me, "hits", "player")) {
        send(other.id, "damage", #{ amount: prop(me, "damage", 1), from: me });
        let c = center(me);
        particles("hit", c[0], c[1]);
        destroy(me);
    }
}
