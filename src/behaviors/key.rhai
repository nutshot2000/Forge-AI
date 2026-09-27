// Behavior: key
// A key: a player touching it adds one to game("keys") (kept across levels). Doors with
// needs_key use keys up.
// Props: by ("player")

fn on_touch(me, other) {
    if !has_tag(other.id, prop(me, "by", "player")) { return; }
    set_game("keys", game("keys", 0) + 1);
    sfx("powerup");
    emit("key_picked", #{ keys: game("keys", 0) });
    destroy(me);
}
