// Behavior: goal
// The level exit: when a player touches it, go to the `next` level (keeping game-wide values
// like score), or show a win screen if there's no next level.
// Props: next (level name, optional), title ("YOU WIN!"), text

fn on_touch(me, other) {
    if !has_tag(other.id, "player") || state("won", false) { return; }
    set_state("won", true);
    sfx("win");
    emit("goal", #{ id: me });
    let next = prop(me, "next", "");
    if next != "" {
        goto_level(next);
    } else {
        show_screen(prop(me, "title", "YOU WIN!"), prop(me, "text", "Score: " + game("score", 0)), "");
    }
}
