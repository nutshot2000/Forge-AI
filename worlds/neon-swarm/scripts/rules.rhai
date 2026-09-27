// Title screen, waves, HUD, game over.
fn rules() {
    if !state("started", false) {
        if !state("title_shown", false) {
            set_state("title_shown", true);
            set_state("paused", true);
            show_screen("NEON SWARM", "Move: WASD / arrows    Aim: mouse    Fire: hold left click", "");
            ui("start", #{ type: "button", text: "START", x: 50, y: 72, anchor: "center", size: 5 });
        }
        return;
    }
    let players = tagged("player");
    if players.is_empty() { return; }
    let p = players[0];
    ui_set("hp", "value", max(0, p.hp));
    ui_set("score", "text", "SCORE " + game("score", 0));
    ui_set("wave", "text", "WAVE " + state("wave", 0));
    if state("over", false) {
        if !state("restart_shown", false) {
            set_state("restart_shown", true);
            ui("restart", #{ type: "button", text: "PLAY AGAIN", x: 50, y: 72, anchor: "center", size: 5 });
            ui("final", #{ type: "text", text: "Score " + game("score", 0) + "  ·  Wave " + state("wave", 0), x: 50, y: 58, anchor: "center", size: 4, color: "#7ee8ff" });
        }
        return;
    }
    // When the arena is clear, the next wave comes.
    if count("drone") + count("brute") + count("sniper") == 0 && !state("wave_pending", false) {
        set_state("wave_pending", true);
        after(-1, 60, "wave");
    }
}

fn on_timer(name, data) {
    if name == "wave" {
        let wave = state("wave", 0) + 1;
        set_state("wave", wave);
        set_state("wave_pending", false);
        ui("banner", #{ type: "text", text: "WAVE " + wave, x: 50, y: 40, anchor: "center", size: 9, color: "#7ee8ff", font: "mono" });
        after(-1, 90, "banner_off");
        sfx("wave");
        let n = 3 + wave * 2;
        for i in 0..n { spawn_enemy(wave, i); }
    }
    if name == "banner_off" { ui_remove("banner"); }
}

fn spawn_enemy(wave, i) {
    // Around the edges of the arena.
    let side = rand(4);
    let x = if side == 0 { 1.5 } else if side == 1 { width() - 2.5 } else { 2 + rand(width() - 4) };
    let y = if side == 2 { 1.5 } else if side == 3 { height() - 2.5 } else { 2 + rand(height() - 4) };
    let kind = if wave >= 3 && i % 4 == 3 { "brute" } else if wave >= 2 && i % 5 == 4 { "sniper" } else { "drone" };
    create_prefab(kind, x, y);
    particles("sparkle", x + 0.5, y + 0.5);
}

fn on_ui(name) {
    if name == "start" {
        ui_remove("start");
        hide_screen();
        set_state("started", true);
        set_state("paused", false);
        set_game("score", 0);
        fade_in(30);
        sfx("blip");
        ui("hp_label", #{ type: "text", text: "HULL", x: 2, y: 2, size: 2.6, font: "mono", color: "#9fb8ff" });
        ui("hp", #{ type: "bar", x: 2, y: 5.5, w: 20, h: 2.6, value: 5, max: 5 });
        ui("score", #{ type: "text", text: "SCORE 0", x: 98, y: 2, anchor: "topright", size: 4, font: "mono" });
        ui("wave", #{ type: "text", text: "WAVE 0", x: 50, y: 2, anchor: "top", size: 3.5, font: "mono", color: "#7ee8ff" });
    }
    if name == "restart" { goto_level(level()); }
}
