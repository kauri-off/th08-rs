# IMPL130 questions

Answered in [answers_re_q57](../answers_re_q57.md).

1. `src/bullet.rs` `cancel_drop` (08 §7 / §8.3): the cancels pass the raw player+0xE2A90 to `ItemMgr_Spawn`. An
   area item of −1 ("none", 09 §8 area +0x28) therefore reaches `ItemMgr_Spawn` with type −1. 08 §8.3 does not say
   what the spawn does with a negative type (the u8 store gives 0xFF, script 0x3D + 0xFF). The port's spawn returns
   no item for type < 0 (pre-existing guard). What does the original do?

   **Answer ([RE-Q57](../answers_re_q57.md), 08 §8.3 "No type check", §8.2 row −1):** the original spawns it. No type guard: the slot is
   taken by the normal search, type byte 0xFF, the given mode (1: no RNG), VM script −1 + 0x3D = 0x3C (int add,
   0x4403BC). Collection has no case for 0xFF (stats bit, sound 0x15, free only); it can fall out (rank −3); the draw
   sign-extends the type (sprites 0xAB / 0xB5). The port's `kind < 0` guard in `item.rs` `spawn` is a CODE BUG.
