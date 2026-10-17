# Open questions for the dirty room (feed into the next RE briefs)

From impl2 (ANM VM):
- Main menu: which title01.anm scripts it starts, draw order, highlighted/normal sprite switching, interrupts sent. (→ menu/flow RE)
- RNG algorithm + seed. (→ 11 RNG)
- Where/when th08logo.jpg is shown.

From impl5 (ECL):
- Mode-2 mirroring: velocity negated then subtracted (cancels) → only op 64 delta mirrored; op 66 fairies unmirrored. Intended?
- Enemy draw mode and per-layer split between draw prio 8 and 11; enemy VM position vs final_pos.
- Section-request opcode: 06 says 0x92 (=146), 07 says 147 — which?
- Random-direction helpers 0x422020 / 0x4224A0 exact branches; op 169 constants.
- Timeline stall (ops 7/13): decrement-and-return or net-zero for the clock?
- Interrupts: copy argument registers like calls? Main-context return at depth 0?
- Starting rank value of a new game.
- Death-sub jump from kill_all / threshold: is the death sub cleared?
- Player spawn position (impl uses 192,384).

From impl6 (bullets/items):
- Op 111: 07 says the ints are "copied raw", but 123 of 582 op-111 instructions have var-mask bits on c/d (e.g. `$10000` as the 0x20000 wait or the 0x4000 sprite offset). Are a–d variable-resolved (impl: yes)?
- Bullet draw: which draw mode, z, and is VM0's rotation set from the bullet angle (impl: mode 2, rotation = angle + π/2)? Same for item VMs (impl: mode 2, z 0.15) and laser body/emitter (mode 2).
- Player_GrazeTest / Player_HitTest: player-state conditions (impl: no graze in states 1/2; hit only in state 0) and whether the hit test returns 1 for an invulnerable player (bullet cancelled?).
- Laser state 1: is the collision tested every frame with graze only on t % 20 == 0, or only tested on those frames (impl: the former)? Laser graze box (+48) vs the player's hitbox or graze box? Where do laser-graze time orbs spawn?
- Laser cancel: are the "one item per 32 px" items spawned only for lasers that were in state < 2? Item type/mode (impl: 6, mode 1)?
- CancelBonus: arguments used on a boss death without a spell card (impl: cap 8000, lasers too); does it set the cancel guard?
- Cancel modes used by spell-card start, timeout and the full-power item (impl: 1).
- Enemy drops: when is a drop "auto-collected" (mode 1)? Item mode of the cancel-bonus stars in kill_all?
- Point item: the doubling threshold function 0x406D10; "rank +3 (+10 if v ≥ max)" — +10 instead of +3 or in addition? Converted point (8): exact rounding order.
- Ex cmd 0x10/0x20/redirect start: are their block timers reset to 0 at start (impl: yes)?
- Natives 12/14: tag semantics (impl: tag 1 → ghost/tag 0, tag 0 → normal/tag 1), var 10016/10017 as angle/speed, native 14's intermediate step; native 12's chain/stage part.
- Item draw at y < −8: alpha formula uses (8 − y) as written? Arrow drawn at y = 8 + 16 (screen) — confirm.
- Rank: items/graze/death change "rank" by +1/+3/−3/+6/+200/−1600 — how does that map to ECL var 10041 (0–32)? Starting max point value (stats +0x24), lives/bombs.
- Time-orb timers 0x18B89BC (state 3 → 1 when < 0) and 0x18B89D4 (gauge change gate): who sets them?
- Which bomb-count changes call GameMgr_UpdateChecksum (09 §9.4 lists 0x439883 but not 0x406F30)?

## From impl7-player
- Bomb choreography for all 16 bombs: object positions/speeds/spawn timing and ANM scripts.
- Option draw scripts and behaviour of 0x44E3A0 (attack), 0x44EE70, 0x44F2D0, 0x44F5E0; Remilia solo options, Youmu's half, Yukari's offset.
- Player laser (Alice) update funcs 4/5: length, trail, damage rects, end condition.
- Shot sprite rotation vs VM tag; youkai red-tint colour; focused hitbox marker sprite.
- Enemy contact: which of 0x44A470 / 0x44A360 uses 0.7 vs 1.5 box; the "−10 life on second test".
- How EnemyManager fills homing targets +0xE2AA4/+0xE2AB0/+0xE2ABC.
- Bomb damage rules during spell cards (impl: d/7).
- Bomb-allowed counter +0xE2A68 at load; last-spell index "1 − focus, +2"; starting lives (impl: 2).
- Input bits for Esc and Ctrl.
