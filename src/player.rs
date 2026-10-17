// SPEC-GAP: no player yet (09 spec); every aiming/distance query reads this fixed point (bottom-centre of the playfield).
pub fn position() -> [f32; 3] {
    [192.0, 384.0, 0.0]
}

// Player power (< 128 selects power items in drops); constant until the player exists.
pub fn power() -> i32 {
    0
}
