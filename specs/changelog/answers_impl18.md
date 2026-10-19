# Answers to IMPL18 questions (RE-Q18)

Spec sections changed:
- 07 §10.1 step 6 (hitByBomb rule), step 7 (hit-flash counter, draw-list prepend and which enemies are linked, aimed
  pointer vs death check), §10.6 (list walk order); the step-4 screen check is covered in answers_impl19.
- 14 §3.1 (aimed enemy: same-frame kill ordering, golden 1431–1433).
- 09 §3 (previous vx +0xE2A9C), 09 §7 "Gauge while bombing" (no add on the start frame, add on the end frame).
- 12 §2.2 (menu background copy vs batched nowloading quads; description VM on the loader release frame),
  12 §2.4 (character-select cursor).
- answers_phase5 "Drops auto-collected" row: marked CORRECTED.

Runtime data: `specs/data/q18/` (decoded probe dumps, see §Tools). Tool: `TH08_SCAN="P0 P1"` session scan
(TOOLING_REPORT §12).

## Q1. golden frame 1451 / option attack at 1432 — VERIFIED (runtime + code)

The aimed enemy at frames 1431–1432 is **slot 7**, an enemy that lived for exactly one frame:
- Spawned by the timeline on frame 1431 at final = base = (284.04, −21.73) (flags A 0x0001204D after death,
  flags B 0x38, timer 1, no parent). x is within 64 px of the player (285.75).
- Killed in its first update by shots: life 20 → 0, score 30265 → 30329 and gauge +210 on frame 1431 (the trace
  shows this kill, so the port also spawns and kills it; enemy count stays 7).
- Post-calc of frame 1431: player +0xE2ABC still points at slot 7 (dump `golden_aimed_slot7_f1431.txt`: alive bit
  clear, life 0).

Why the pointer survives: in `EnemyMgr_Calc` the aimed test (|final x − p.x| < 64, no parent, pointer 0 or
final y < aimed base y) is in the damage step, **before** the death check. Death mode 0 only clears the alive bit;
it does not call `Enemy_Free`, so it does not clear the pointer.

Sequence:

| frame | what happens |
|---|---|
| 1431 | slot 7 is spawned, becomes the aimed enemy, then dies |
| 1432 | player calc (prio 9, before the enemies at prio 11): the Yukari option sees the pointer, shot timer ≥ 0, option timer ≥ 10 → interrupt 3, sub-state 3, one attack step toward (284.0, −21.7) + (0, 32) (y ≥ 32). Then `EnemyMgr_Calc` finds slot 7 inactive and clears the pointer |
| 1433 | sub-state 3 with no enemy → interrupt 1, sub-state 0, pointer cleared |

Answers to the sub-questions:
- Slot 7 is not "hidden": it is a normal one-frame enemy.
- The e7/e8 spawn times are not earlier than in the port. The original's slot 7 at 1441 (273.8, −21.7) and slot 8 at
  1451 (263.7, −21.7) are later members of the same 10-frame column.
- The 64-px test is against the player's x, as specified.

Port fix: do the aimed update before the death check, and do not clear the pointer on a mode-0 death. The pointer
is only cleared by `Enemy_Free`, by the next frame's inactive-slot test, or by the option itself.
Data: `golden_scan_f1398-1468.txt` (per frame: F player/aimed slot/shot timer, O option raw +0x2A4…, E enemies,
I items).

## Q2. selftest1 item drop mode during the bomb — VERIFIED (runtime + code)

- +0xFDC = "bombing". `Player_UpdateBomb` sets it to 1 at the bomb start and to 0 at the end; it stays 1 for the
  whole duration (dump: fdc 1, fe4 150 throughout frames 680–716).
- `EnemyMgr_Calc` initialises hitByBomb from +0xFDC, but `Player_DamageAgainst` overwrites the out flag. When the
  player state timer (+0xE2AF4) has advanced this frame (current ≠ previous; true on every normal frame) it:
  - writes 0 at entry;
  - writes 1 only if one of the first 192 damage areas (+0xB8834) is active, has life % period == 0, overlaps
    the enemy box on this call, and +0xFDC ≠ 0.
- When the enemy has an extra shot box, the second call overwrites the flag again.
- The drop mode is this final flag, so mode 1 = the enemy was hit by a firing bomb area on the killing call.
- Runtime: the item in the question is the one dropped at **frame 698** (poll 1312). In the draw log it is present
  1326 = poll + 14, which is why it looked like row 1326.
  - Enemy slot 2 (life 4, at (234.6, 212.4)) was killed by shots.
  - The Yukari rings at that frame are centred on the player (256, 384) with radii 126/116/106 (distance ≈ 173,
    no overlap).
  - The item spawned as type 0 in state 0 (mode 0) at (235.28, 211.44) with vel.y −2.173, and rises.
- The port's mode 1 there auto-collects it, which gives the +1 power seen at row 1326.
- Data: `selftest1_scan_f680-716.txt` (A lines = active bomb areas, 16 raw dwords: x, y, r, dr, w, h, dw, dh,
  angle, life (+0x24), item, damage (+0x2C), hits, max, period (+0x38), active).

## Q3. Smaller items

- **Enemy draw lists** (VERIFIED, code). Each processed enemy is **prepended** to the list of its layer at the end
  of its loop iteration, if it is still alive and bit 4 is clear. The heads are cleared at the start of
  `EnemyMgr_Calc`, before the timelines. Draw walks from the head, i.e. reverse processing order.
  - Linked in a frame: every enemy the slot loop processes in that frame. Timeline spawns run before the loop, so
    they are processed (timer 1 after the spawn frame, e.g. golden slot 7 at 1441) and linked in their spawn frame.
  - Enemies spawned by another enemy's ECL during the loop: linked the same frame if their slot is higher than the
    spawner's, from the next frame if it is lower.
  - Not linked: freed enemies, mode-0 deaths, bit-4 enemies (e.g. no sprite, see answers_impl19 §Q2).
  - The port's "link at the end of each enemy's calc, draw backwards" is equivalent.
- **Hit flash** (VERIFIED, code). Counter byte e+0x3314:
  - counter 0 and damage > 0 this frame → flash, sfx 0x14/0x25, counter := 1;
  - else counter ≠ 0 → counter −= 1, no flash, no sfx;
  - else no flash.

  Continuous hits flash on every 2nd frame. The port's "skip when the previous frame flashed" is the same.
  Youkai-mode enemies (bit 11) use the permanent tint branch instead and do not touch the counter.
- **Bomb gauge** (VERIFIED, code). Correct: there is no add on the start frame. The add is also done on the **end**
  frame: the "timer ≥ duration" end branch runs, then the add still happens with the old index. The add is forced
  (not suppressed by bombing).
- **Focus switch while moving** (VERIFIED, code).
  - The original stores the previous vx as a float at player +0xE2A9C, written at the end of movement.
  - The team focus-switch reset (script 0 or 5) also writes +0xE2A9C := 0.
  - The port's "previous vx = 0 after a switch" is exact.
- **Character-select cursor** (VERIFIED, code).
  - The difficulty confirm does `Menu_SetScreen(5/9/13)`, then cursor := 0.
  - The character screen's entry block (its first calc, +0x1442C == 0) sets cursor := g_ShotType before any input.
  - The trace's cursor column is sampled at `MainMenu_Calc` entry, so it shows the 0 left by the difficulty
    screen.
- **Description VM at the loader release** (VERIFIED, code + selftest1 presents 46–48).
  - The menu loader tail (session park point) sets +0xC2BC = description VM 0, +0x14434 = 0 and calls
    `Loading_End`, all before the release frame's calc.
  - So that frame's `MainMenu_Calc` already runs the Top handler and `AnmVm_Run(+0xC2BC)`. The description VM's
    first tick is in the release frame (present 46, not drawn: the loading screen still shows).
  - Present 47 shows t = 2: description alpha 0x19 = 255·2/20, title01 VMs 0x1E.
  - No interrupt 1 is needed: script 9 fades in by itself; interrupt 1 is only sent on later cursor changes.
  - The port starts the description VM one calc late.
- **Title surface order** (VERIFIED, code + draw log). Surface 0 is drawn by the **menu's** draw (prio 3,
  0x47087F): `Anm_DrawSurface(0)` comes before its VMs.
  - The nowloading quads from `Supervisor_DrawLoadingVms` (prio 2) are only queued in the sprite batch.
    `Anm_DrawSurface` is an immediate CopyRects and does not flush the batch.
  - So the device gets the copy first and the nowloading quads when the batch is flushed by the first title01
    quad's state change. Visually the nowloading sprites lie on top of title00.
  - The port must copy surface 0 before drawing the prio-2 loading VMs, or defer sprite submission the same way.

## Tools
- `re/probe/session.inc`:
  - `TH08_SCAN="P0 P1"` (session play/record, polls P0..P1, scene 2): after each calc, `scan.txt` in the output
    dir with F / O / X / E / S / A / I lines.
  - `TH08_LDLOG=1`: `ldlog.txt` in the game dir, logging ANM preloads, post-load steps and surface loads with
    poll, present and loading-frame counter.
- `re/work/runs/q18/scanrun.sh <session> <out> <p0> <p1>`: partial replay that stops after p1.
- `re/work/runs/q18/scandec.py`: hex → float decoding.
