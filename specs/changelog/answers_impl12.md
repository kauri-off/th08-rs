# Answers to IMPL12_QUESTIONS (dirty room, 2026-10-18)

All answers were read from th08.exe (`re/extracted/all_decomp.c`, disassembly where noted).

## 1. `Enemy_KillChildren` path choice — VERIFIED

The function runs two independent blocks, in this order:

1. **Chain block**: runs when `n = Enemy_ChainChildCount(e)` ≠ 0. `Enemy_ChainChildCount` (0x41FD40) returns
   0 unless `Enemy_IsChainRoot(e)` (0x41F000) holds. That test is **parent (+0x2DA4) == 0 and +8 ≠ 0**. Only then
   does it count the +8 links. A chained child always has a parent pointer, so its count is 0 and it never
   chain-kills its later siblings, whether reward is 0 or 1.
2. **Single block**: runs when `Enemy_HasParent(e)` (parent ≠ 0) and reward ≠ 0 (§2 of answers_impl10).

Then `Enemy_ChainUnlink(e)` runs in every case. A child that is freed or times out (reward 0) only unlinks itself:
prev+8 := next, next+4 := prev. Its siblings stay chained to the parent.
The port's rule ("chain-kill only from enemies without a parent") is correct.

## 2. Parent counter +0x3380 — VERIFIED (displacement scan, all accesses)

Writers: ECL ops 90–92 (+1 per child created; 0x41B0DE/0x41B28E/0x41B4AF), and `EnemyMgr_Calc` 0x42D63A
(−1 when an enemy **with a parent** dies by damage, just before its `Enemy_KillChildren(e, 1)`). Nothing else
writes it. `Enemy_KillChildren` reads it but never resets or decrements it.
After a chain kill the children have parent = 0. When they die later, there is no decrement. Children removed by
timeout or free are not decremented either. So the counter keeps the number of children created minus the
number of children killed individually while still chained.
The enemy slot is zeroed when a new enemy is created, so the counter starts at 0.
HUD "+p3(p4)" (`Hud_DrawBoss`, 12 §5.4): p3 = `Enemy_ChainChildCount(boss)` (live chained children), p4 = boss
+0x3380. "+0(3)" after the children died is the expected original output.

## 3. Boss part (step 5) gate — VERIFIED

Gated only on **reward ≠ 0** inside the chain block, i.e. e is a chain root with ≥ 1 child. The boss flag
(flag A bit 1) is **not** tested there. It only selects the cancel-circle type of each child (7 vs 9, step 3).
The 2× popup shows e+0x3380 and uses the root's own +0x2D88. The 2·(+0x3380) orbs, the cancel circle (32, 1.0, 16,
7) and the `Timer_Set(0x18B89D4, 0)` follow.

## 4. Orb count `c` vs the ÷3 — VERIFIED

Per child, the order is: cancel type → `c` from the **current** n (team/human/youkai formula) → if bombing,
n := n / 3 → `Popup_Large(…, n, …)` → c orbs. So `c` uses n before this child's division, and the popup shows the
divided n. The next child starts from the already divided n, so the division compounds (as in answers_impl10).
The popup colour is chosen once, before the loop, from the undivided n. Same as the port.

## 5. GameFlags bit 13 box (`EnemyMgr_Calc`) — VERIFIED

Condition: not GameFlags bit 10, GameFlags bit 13 set, and boss slot 0 (EnemyMgr+0x9DCDA0 = 0xF54CC0) ≠ 0.
Call: `Player_DamageAgainst(pos = (192, 224, 0), size = (384, 448, 0), accumulator = boss0 + 0x2E10, out = scratch)`.
This box is the **whole playfield** (centre/size convention of 09 §4.5).
The **returned damage is discarded** (the call result is not stored), so the box deals **no damage**. It exists for
its side effects:
- every player bullet anywhere in the playfield is treated as hitting: hit animation and state 2, as in 09 §4.5;
- the boss's time-orb accumulator (+0x2E10) grows with the shot damage, so time orbs spawn at human max gauge;
- damage areas that overlap are ticked: dealt total, cap, and the hit-by-bomb output go to the scratch variable.

Bit 13 is set by ECL op 176 (last spell) on stages 6A/6B and on the practice cards listed in 07 op 176. It is
cleared at the next op 176 and at the clear points in 11.

## 6. Native 22 `Spell_ShowNameOnly` (0x415D10) — VERIFIED

Native 22 calls `Spell_ShowNameOnly(g_Gui, "「リザレクション」")`. The name is a **fixed string in the exe**
(0x4B495C, cp932 「リザレクション」), not an ECL argument. The function:
1. sets gui flags (0x4EA670) bit 10 (0x400), **clears bit 0 (card active) and bit 4**;
2. calls `Gui_ShowBossSpellName(face = −1, name, sprite arg = 1)`.

With face −1 the face/cut-in VM (+0x3C4) is not touched: no new portrait, no cut-in.
Bit 10 suppresses the interrupt 1 to VM +0x2374, the spell background/strip VM of 12 §5.8. So only the name
plate (+0x668/+0xBB0 scripts 1/3, sprite 1), the text VM +0x139C (script 5, or 6/7/8 for practice cards
0xCD–0xDD) and the plate interrupt (+0x1E2C ← 1) play. SE 0xE plays and the panel full-repaint counter is set to 2
(§9).
So yes, it ends the active card's "active" state (bit 0) while the card's own timers keep running. Bits 2
(capturable) and 3 (failed) are left alone. A later `Spell_End` sees bit 0 clear and does nothing (no bonus).
Spell_Start clears bit 10 again.

## 7. Spell history counters (CATK) — VERIFIED (12 §5.8 digits; 15 §2.5)

- **Attempts**: `Spell_Start` 0x4152A0, every time a card starts, **including timeouts and survival cards and
  practice**, but **not during replay playback** (`GameMgr_IsReplay`).
  - Story mode (GameFlags bit 14 clear) increments `CATK[id].story_attempts[shot]` (+0x124 + 4·shot) and the
    "all shots" slot [12].
  - Spell practice (bit 14) increments `practice_attempts[shot]` (+0x1C0) and [12] instead.
  - Each counter is capped at 9999. If the card's checksum (+0x0E) is wrong at that moment, all six counter arrays of
    the card are zeroed first. The difficulty byte +0x0F := the current difficulty (0x160F538).
- **Captures**: `Spell_End` 0x4161B0, only when the card is still active (bit 0), not failed (bit 3 clear) and still
  capturable (bit 2), and not during replay playback.
  - Story increments `story_captures[shot]` (+0x158) and [12] (= 0x160F6A0/0x160F6D0 per card); practice
    increments `practice_captures[shot]` (+0x1F4) and [12]. Both are capped at 9999.
  - Max bonus per shot is updated as well (+0xF0 / +0x18C).
- **Timeouts**: `Enemy_CheckTimeout` 0x42B930 does the following unless enemy flag A bit 27
  (op 155 "fixed bonus", the survival cards): it calls 0x42BC50, which clears bit 2, sets bit 3 and zeroes the
  bonus +0xFC.
  - A normal card that times out is therefore an attempt without a capture.
  - A survival card (bit 27) that times out without miss or bomb is captured.
- **Per difficulty**: not separately. Each difficulty has its own card ids (e.g. the four versions of a card are
  four CATK entries), so the counters are per card id × shot. The plate shows the entry for the current card id
  (12 §5.8). A port without score.dat counts per session the same way: key (card id, shot), story and practice
  kept apart.

## 8. Spell-practice effect file table — VERIFIED

`EffectMgr_OnRegister` indexes `0x4C7144 + id·4` only for id ≥ 0xD8. 0x4C7144 + 0xD8·4 = 0x4C74A4, which is
the slot just after the 9-entry stage table 0x4C7480. So the used part is a 6-entry table:

| card id | effect file |
|---|---|
| 216 (0xD8) | `eff09sk.anm` |
| 217 | `eff09ym.anm` |
| 218 | `eff09al.anm` |
| 219 | `eff09rm.anm` |
| 220 | `eff09yy.anm` |
| 221 | `eff09yk.anm` |

Stage table 0x4C7480 (ids < 0xD8 or not spell practice): `eff01 eff02 eff03 eff04a eff04b eff05 eff06 eff07
eff08` (.anm) by stage index. The anm goes to ANM slot 9.

## 9. Global 0x17CE8CC := 2 — VERIFIED

This is the **side-panel full-repaint counter** of 12 §5.3. Readers:
- `Hud_DrawPanel`: while it is ≠ 0, the panel background tiles and strips are repainted in full;
- `Hud_Calc`;
- `Frame_Present`: decrements it once per presented frame while the game is not paused (0x164D0BA == 0).

Writers (value = number of full-repaint frames):
- 2: `Gui_ShowPlayerSpellName` / `Gui_ShowBossSpellName` (spell-name plates), `Hud_ShowBonus`, `Hud_ShowMessage`,
  `Hud_ShowSpellBonus`, `Frame_Present` after a failed Present + device reset;
- 3: WinMain after a device reset;
- 8: `ContinueMenu_Update`; `Hud_Calc` raises it to 8 while the Skip key (input 0x100) is held;
- 0x10: `Hud_Load`; 0x3C: `GameManager_StageStart`; 0x1BA: `Msg_Run` (one dialogue op);
- 0: `Supervisor_Init`.

It exists because the original keeps the back buffer between frames (copy swap; see `specs/data/drawlog/FORMAT.md`
§4) and clears only the playfield. Plates that slide over the panel would otherwise leave trails. A port that
redraws the whole screen every frame can ignore it.
