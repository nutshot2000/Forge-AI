// Global rules, run once per tick after every entity.
fn rules() {
    if count("hero") > 0 && count("coin") == 0 && !state("won", false) {
        set_state("won", true);
        emit("win", #{ tick: now() });
    }
}
