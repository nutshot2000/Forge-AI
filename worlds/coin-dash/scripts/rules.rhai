// Runs once per tick after every entity: HUD, winning and losing.
fn rules() {
    let hs = find("hero");
    if hs.is_empty() { return; }
    let h = hs[0];
    let hearts = "";
    for i in 0..h.hp { hearts += "♥"; }
    hud("hp", "HP   " + hearts);
    hud("gold", "Gold " + h.gold + " / " + state("total", 0));
    if h.hp <= 0 && !state("over", false) {
        set_state("over", true);
        hud("msg", "GAME OVER · press Undo to try again");
        emit("game_over");
    }
    if count("coin") == 0 && !state("won", false) {
        set_state("won", true);
        hud("msg", "YOU WIN! Every coin collected");
        emit("win");
    }
}
