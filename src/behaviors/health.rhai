// Behavior: health
// Hit points. Takes "damage" messages ({amount}) and "heal" messages ({amount}); flashes and
// is briefly invulnerable after a hit. At 0 hp it emits "died": players (tagged "player")
// get a game-over screen, anything else is removed.
// Props: hp (3), max_hp (hp), invuln_ticks (45), on_death ("destroy" | "respawn" | "game_over")

fn tick(me) {
    let i = prop(me, "invuln", 0);
    if i > 0 { set(me, "invuln", i - 1); }
}

fn on_message(me, msg, data) {
    let amount = if type_of(data) == "map" && "amount" in data { data.amount } else { 1 };
    if msg == "heal" {
        set(me, "hp", min(prop(me, "hp", 3) + amount, prop(me, "max_hp", prop(me, "hp", 3))));
        sfx("powerup");
        return;
    }
    if msg != "damage" || prop(me, "invuln", 0) > 0 { return; }
    let hp = prop(me, "hp", 3) - amount;
    set(me, "hp", hp);
    set(me, "invuln", prop(me, "invuln_ticks", 45));
    set(me, "alpha", 0.3);
    tween(me, "alpha", 1.0, 20);
    sfx("hit");
    emit("damaged", #{ id: me, hp: hp });
    if hp > 0 { return; }
    emit("died", #{ id: me, kind: get(me).kind });
    let mode = prop(me, "on_death", if has_tag(me, "player") { "game_over" } else { "destroy" });
    if mode == "game_over" {
        set_state("over", true);
        show_screen("GAME OVER", "", "Undo or restart to try again");
    } else if mode == "respawn" {
        let home = prop(me, "spawn", [get(me).x, get(me).y]);
        set(me, "x", home[0]);
        set(me, "y", home[1]);
        set(me, "hp", prop(me, "max_hp", 3));
    } else {
        destroy(me);
    }
}
