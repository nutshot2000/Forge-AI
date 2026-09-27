// Behavior: door
// A solid door. Opens on an "open" message (from a button, a lever, a script...), closes on
// "close", flips on "toggle". A door with needs_key opens when a player touches it while
// holding a key (game("keys") > 0), using one up.
// Props: open (false), needs_key (false)

fn tick(me) {
    // Keep solidity in sync with open/closed (also right after the door is placed).
    let open = prop(me, "open", false);
    let solid_now = prop(me, "solid", false);
    if open && solid_now != false { set(me, "solid", false); }
    if !open && solid_now != true { set(me, "solid", true); }
}

fn on_message(me, msg, data) {
    if msg == "open" { set_open(me, true); }
    if msg == "close" { set_open(me, false); }
    if msg == "toggle" { set_open(me, !prop(me, "open", false)); }
}

fn on_touch(me, other) {
    if prop(me, "open", false) || !prop(me, "needs_key", false) || !has_tag(other.id, "player") { return; }
    let keys = game("keys", 0);
    if keys > 0 {
        set_game("keys", keys - 1);
        set_open(me, true);
    }
}

fn set_open(me, open) {
    if prop(me, "open", false) == open { return; }
    set(me, "open", open);
    set(me, "solid", !open);
    tween(me, "alpha", if open { 0.25 } else { 1.0 }, 12);
    sfx(if open { "powerup" } else { "blip" });
    emit(if open { "door_opened" } else { "door_closed" }, #{ id: me });
}
