// The player's gun: aims at the mouse, fires while the left button (or Space) is held.
fn tick(me) {
    if !state("started", false) || state("over", false) { return; }
    let mx = mouse_x();
    if mx >= 0.0 { set(me, "angle", angle_to_point(me, mx, mouse_y())); }
    let cd = prop(me, "cooldown", 0);
    if cd > 0 { set(me, "cooldown", cd - 1); return; }
    if mouse_down("left") || key("space") {
        let c = center(me);
        let a = prop(me, "angle", 0.0);
        let v = vel_from_angle(a, 18.0);
        let b = create_prefab("shot", c[0] - 0.125 + v[0] * 0.03, c[1] - 0.125 + v[1] * 0.03, #{ from: me, angle: a });
        set_vel(b, v[0], v[1]);
        set(me, "cooldown", 6);
        sfx("laser");
    }
}
