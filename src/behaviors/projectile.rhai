// Behavior: projectile
// Flies until it hits a wall, runs out of time, or touches its target; then it's gone.
// On hitting an entity tagged `hits`, sends it a "damage" message (see the health behavior).
// Use with physics: true and gravity: 0 (or gravity > 0 for arcs); set its velocity when created.
// Props: damage (1), hits ("player"), life (180 ticks), from (who fired it; never hits them)

fn tick(me) {
    if prop(me, "hit_wall", 0) != 0 || prop(me, "hit_ceiling", false) || prop(me, "on_ground", false) {
        destroy(me);
        return;
    }
    let life = prop(me, "life", 180) - 1;
    if life <= 0 { destroy(me); return; }
    set(me, "life", life);
}

fn on_touch(me, other) {
    if other.id == prop(me, "from", -1) { return; }
    if has_tag(other.id, prop(me, "hits", "player")) {
        send(other.id, "damage", #{ amount: prop(me, "damage", 1), from: me });
        destroy(me);
    }
}
