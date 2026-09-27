// Enemies: take damage (flash + number), explode and score when destroyed, and hurt the
// player by ramming them.
fn on_message(me, msg, data) {
    if msg != "damage" { return; }
    let amount = if type_of(data) == "map" && "amount" in data { data.amount } else { 1 };
    let hp = prop(me, "hp", 1) - amount;
    let c = center(me);
    float_text(c[0], c[1] - 0.5, "-" + amount, "#ffd060");
    if hp > 0 {
        set(me, "hp", hp);
        set(me, "alpha", 0.35);
        tween(me, "alpha", 1.0, 8);
        sfx("hit");
        return;
    }
    particles("explosion", c[0], c[1]);
    camera_shake(0.2);
    freeze(2);
    sfx("boom");
    let pts = prop(me, "points", 10);
    set_game("score", game("score", 0) + pts);
    float_text(c[0], c[1], "+" + pts, "#5fd39a");
    destroy(me);
}

fn on_touch(me, other) {
    if has_tag(other.id, "player") && !state("over", false) {
        send(other.id, "damage", #{ amount: 1 });
        let c = center(me);
        particles("explosion", c[0], c[1], #{ color: "#ff5f6d", count: 20 });
        camera_shake(0.5);
        flash(10, "#ff3048");
        destroy(me);
    }
}
