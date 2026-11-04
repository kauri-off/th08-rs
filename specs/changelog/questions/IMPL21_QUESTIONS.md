# IMPL21 questions

Answered in [answers_impl21](../answers_impl21.md).

Tools: `session <dir> [--to N]` (`TH08_SESSION_DUMP=a-b`, `TH08_SESSION_RNG=<row>`), `demosync <n> [a-b] [rng-frame]`
(`TH08_BDUMP=1`). Row = menu frame (trace column 26).

## Q1. Var 10096 (07 §6 "depth of this enemy in its child chain")
- Demo 3 f1501: root e1 (4 chained children) runs ecldata2 sub 8 `isub3 $10000, 5, $10096` → count1. The original
  fires 3 bullets (count1 1, so 10096 = 4); with "depth" (0 for a root) the port fired 15.
- ecldata2 sub 44 (a root) loops `ijge $10096, 3` (waits while ≥ 3 children remain).
- The port now reads 10096 = number of chain entries after this enemy along the +8 link (SPEC-GAP). A root reads its
  live chained children. Please confirm the exact walk (+8 or +4, alive test?) and what a chained child reads
  (ecldata4b reads it 12 times).
- Also applied from [answers_impl12](../answers_impl12.md) §1: a child freed without reward now unlinks itself. Before, the root's chain kept
  dead children: sub 44 stalled, and fullrun stage 2 took ~8000 rows longer.

## Q2. Yukari option: sub-0 start move vs attack test (14 §3.1)
- golden scan f1462–1464: at 1462 sub 0, aimed 3 set after the calc. 1463: sub 1 (start move, interrupt 2), follow
  step only. 1464: sub 3 and the first attack step. The port went to sub 3 at 1463.
- Port (SPEC-GAP): in sub 0, when v.x ≠ 0 after the follow step, the animation switch replaces the attack test (the
  aimed pointer is still cleared). In f1432 the option was in sub 2 and entered sub 3 directly. Please give the real
  branch order.

## Q3. Pause: flags bit 2 and the frame counter (selftest1 rows 1423+, 1540+)
- Row 1423 (ESC): recorded flags 0x10004 → 0x10000 while paused. Which code clears GameFlags bit 2 on pause, and when
  is it set again?
- After the pause-quit to the menu (row 1540 on), the recorded frame column (0x164D2C8) is 0; the port keeps 809.
  Where is it reset?
- Applied: paused 1 → 2 on the ASCII calc after the first pause draw (11 §2 table). This matches rows 1423–1425.

## Q4. Bullet→item cancel and collection (golden row 4467, demo 0 f4451, demo 1 f3870)
- golden 4467 (boss defeat, 397 bullets): the original collects one more item than the port (points 413/412,
  rng_ctr +4).
- demo 0 f4451: a cancel spawns 5 items in the original and 4 in the port (16 vs 20 draws, all from
  `ItemManager::spawn` in the bullet step).
- demo 1 f3870: after a mass cancel (159 → 32 bullets), the score is 1 lower in the port (×10).
- What decides which cancelled bullets become items? (cap per frame, slot cursor busy, distance, state-2 bullets?)
  And how is the cancel bonus rounded?

## Q5. Demo 2 f2261: chained child e2 fires (port +2 bullets)
- e2 = ecldata3 sub 16, chained child of e1 (sub 14, op91). Its thread sub 15 fires one aimed stack (count 1×2,
  origin (320, 78)) at e2 t 13 in the port. The original fires nothing through 2262.
- Escan flags: the original e2 has 0x01001149, the port 0x0100114D. Sub 16 runs `op81 3`, which should set bit 2
  (contact), but the original has it clear. Do chained children have contact and firing suppressed somewhere?
  Or does the thread's wait differ?
- Disabling fire for all chained children was tried and rejected (demos 0/2 break early).

## Q6. fullrun_noclip row 6264 (stage 2): extra item spawn in a chain kill
- The port draws 4 more RNG values (one `ItemManager::spawn` from `kill_chain` in `death`), and the score is +10.
  The original draws 16 sparks + 16 death sparks. Which item does the original skip here (boss present, no card)?

## Q7. Enemy trail off-screen test
- 07 §10.1 step 4: an enemy with an after-image trail needs the oldest trail pos off screen too. The port has no
  trail history (op 157 only stores the settings). Please give the ring buffer layout (e+0x3394…, size, write
  order) so it can be modelled.
