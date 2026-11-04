# IMPL24 questions (comment pass batch A: ecl / eclvm / enemy / bullet / item / rng)

Answered in [answers_re_q24](../answers_re_q24.md).

No behaviour was changed in this pass. These are places where the code and the numbered specs disagree, or where code
has no spec section behind it. Each one has a `// SPEC-GAP:` (or a "Not implemented" line) at the given place.

## Code vs spec (decide: fix the code, or fix the spec)

1. **Laser cancel width** (`bullet.rs` `cancel_lasers`). 08 §7 says `width (+0x564) := current width (+0x568)`, so the
   fade shrinks from the current size. The port does the opposite (`cur_width := width`). Which direction is right?
2. **Enemy_KillChildren orb counts and cancel circles** (`enemy.rs` `kill_chain`). The per-shot-type orb count
   (team n < 8 ? 2n + 10 : 26, solo human n < 4 ? 6n + 16 : 40, solo youkai n < 10 ? 2n + 6 : 26), the orb
   placement (Rng_Float·2c radius, Rng_Signed·π angle) and the cancel circles (32 px, 2.0 / 8 frames / item 7 or 9,
   boss 1.0 / 16 / 7) appear only in [answers_impl10](../answers_impl10.md) §2. Please move them into 07 §10.5 or 13 §4.
3. **Chain-kill death sound** (`enemy.rs` `kill_chain`). The port picks sfx 2/3 by the child's index in the chain.
   13 §1.4 says "by enemy slot parity". Which one applies inside `Enemy_KillChildren`?
4. **Var 10096 for a chained child** (`eclvm.rs` `chain_after`). 07 §6 evaluates a child on its parent (0 unless the
   parent is a root). The port counts the entries after the reading enemy. Fix the code to 07 §6?
5. **Interrupt dispatch in threads** (`eclvm.rs` `run_thread`). 07 §5.2 tests `pending_interrupt` at the LOOP of
   every context (main and threads). The port tests only in the main context. Can a thread take an interrupt?
6. **Chained child setup order** (`eclvm.rs` `spawn_child`). 07 §10.5 applies bit 8, bit 11, layer, contact off and
   the aura *after* the spawn-time `Ecl_Run`. The port sets everything except contact-off *before* it. So a child's
   time-0 op 159 (layer) or op 81/80 can win in the port. Also not done: aura VM flag bit 17 := flags A bit 2, and the
   aura VM +0x14 negated when e+0x2E0C bit 0 is set.
7. **Native 31** (`eclvm.rs`). 07 §12 says type 3 while 0x17D6ED4 ≠ 0. The port always spawns type 5.
8. **Op 181** (`eclvm.rs`). 07 §7 plays sfx 0x2D only when the counter is < 12. The port plays it always.
9. **Death mode 3 HUD boss display** (`enemy.rs` `death`). 07 §10.3 mode 3 turns the HUD boss display off with no
   boss test. The port only does it for bosses.
10. **Enemy_ChainChildCount** (`enemy.rs` `chain_count`) counts alive members only. 07 §6 says there is no alive test,
    because dead members are always unlinked. This is the same result unless a dead member stays linked. Keep it?

## Uncited code (no spec section)

- `enemy.rs` `spawn_prepare`: `prev_pos` and `final_pos` are set to the spawn position. 07 §4.1 only says pos.
  This affects vars 10085–87 on the first frame.
- `enemy.rs` `kill_non_bosses`, `eclvm.rs` ops 95 / 123: the calling or switching enemy is excluded from KillAll /
  the non-boss kill. 07 §7 / §10.2 / §11.1 do not say this.
- `bullet.rs` `draw`: per-bullet draw has no spec section (z 0.05, RGB forced white, rotation wrap(angle + π/2)
  only for VMs with a tag). 08 §1 only gives the order.
- `enemy.rs` `draw_layer`: the after-image trail (op 157, 07 §10.6) is not drawn. The screen check also skips the
  oldest-trail test (no trail ring yet).

## Spec items not implemented (marked "Not implemented" in the code, not SPEC-GAP)

Timeline op 9 (set_power, 0 uses); native 26 stage interrupts; natives 28/29 bullet and sprite changes; the
spell-practice guard on ops 129/130/133/134; op 164; op 176 bit 13 whole-playfield damage; op 184
(`Spell_SetBonusFrozen`; the per-second bonus decay does not test it).

## Stale comments corrected

- `enemy.rs` `finish_spell` said the payout was made at once. The code queues it, and `step` pays it 130 frames later
  (07 §11.1).
- `eclvm.rs` op 176 said the bit-13 box "has no size". 07 §7 op 176 now gives it (centre (192, 224), size (384, 448)).
- `eclvm.rs` op 36 and var 10096: the SPEC-GAP text now says what 07 §5.6 / §6 specify.
