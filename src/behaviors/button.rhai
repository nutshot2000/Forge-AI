// Behavior: button
// A pressure plate or switch: when something it cares about steps on it, it sends a message
// to its target (e.g. a door). One-shot by default; with toggle it flips each press.
// Props: target (entity id), msg ("open"), toggle (false), by ("player")

fn on_touch(me, other) {
    if !has_tag(other.id, prop(me, "by", "player")) { return; }
    if prop(me, "pressed", false) && !prop(me, "toggle", false) { return; }
    let target = prop(me, "target", -1);
    if !exists(target) {
        emit("button_unwired", #{ id: me });
        return;
    }
    send(target, if prop(me, "toggle", false) { "toggle" } else { prop(me, "msg", "open") });
    set(me, "pressed", true);
    set(me, "alpha", 0.6);
    sfx("blip");
    emit("button_pressed", #{ id: me, target: target });
}
