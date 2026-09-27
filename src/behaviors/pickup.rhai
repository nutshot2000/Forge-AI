// Behavior: pickup
// Collected when an entity tagged "player" touches it: adds `value` to the game-wide counter
// named by `counter` (kept across levels), plays a sound, and disappears.
// Props: counter ("score"), value (1), sound ("coin"), by ("player")

fn on_touch(me, other) {
    if !has_tag(other.id, prop(me, "by", "player")) { return; }
    let counter = prop(me, "counter", "score");
    set_game(counter, game(counter, 0) + prop(me, "value", 1));
    sfx(prop(me, "sound", "coin"));
    let e = get(me);
    emit("picked", #{ kind: e.kind, x: e.x, y: e.y, counter: counter });
    destroy(me);
}
