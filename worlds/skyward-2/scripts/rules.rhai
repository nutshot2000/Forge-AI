// HUD, winning and losing.
fn rules() {
    let hearts = "";
    for i in 0..state("lives", 3) { hearts += "♥"; }
    hud("a_lives", "Lives " + hearts);
    hud("b_coins", "Coins " + state("coins", 0) + " / " + state("total", 0));
    if state("won", false) { hud("c_msg", "You reached the flag! Coins: " + state("coins", 0)); }
    else if state("over", false) { hud("c_msg", "Game over. Press Undo or restart"); }
}
