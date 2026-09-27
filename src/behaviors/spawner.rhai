// Behavior: spawner
// Makes a copy of a prefab every so often, up to a limit alive at once (waves, dispensers).
// Props: spawn (prefab name), rate (120 ticks), max (3 alive), offset ([0, 0] tiles)

fn tick(me) {
    if now() % prop(me, "rate", 120) != 0 { return; }
    let name = prop(me, "spawn", "");
    if name == "" || !has_prefab(name) { return; }
    // Forget children that are gone.
    let alive = [];
    for id in prop(me, "children", []) {
        if exists(id) { alive.push(id); }
    }
    if alive.len() >= prop(me, "max", 3) { set(me, "children", alive); return; }
    let e = get(me);
    let off = prop(me, "offset", [0, 0]);
    let id = create_prefab(name, e.x + off[0], e.y + off[1]);
    alive.push(id);
    set(me, "children", alive);
    emit("spawned", #{ by: me, id: id });
}
