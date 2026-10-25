# RE-Q52 — replay pilot: GameFlags bit 10 group, next full-game divergences

Code: decompile/disassembly of th08.exe (`Player_Die` 0x44AB40, `Bomb_ReimuLS` 0x40C910, `Bomb_Reimu` 0x40C010,
`EnemyMgr_Calc` death block 0x42D9C9–0x42DA59, `Effect_Spawn` 0x425430, `EffUpd_Particle51` 0x4264F0, `Vec3_Dot`
0x40B540, D3DX dispatch thunk 0x476E9F / table 0x4C872C, SSE normalise 0x48EFB0, x87 normalise 0x4791BC).
Traces only to locate. Runs in re/work/runs/q52/: `orig_Dn14`, `orig_Dn14b` (TH08_SCAN + TH08_SCANBUL), `orig_80406`,
`orig_80406b` (TH08_RNGSITE), `orig_80406c`–`f` (new probe option TH08_SCANEFF = effect ring dump slot:type:age,
TH08_SCANEFFSLOT = one slot's world/vel/acc + camera + stored normalised facing + the D3DX table entry/constants);
`scripts/nrm.c` (native SSE vs exact normalise on the reference CPU). Scratch port `portcopy` (q51 copy with D–G,
built with target/re52; src/ untouched) → `b10`, `fixH`, `fixH2`, `fixJ`; changes in `q52_scratch.diff` (also
contains debug dumps TH08_Q52SCAN / TH08_Q52EFF, not part of any fix).

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | Ar02 6634, Dn14 5048, Ro168 6186, 53373 47393: GameFlags bit 10 only in the original | Bit 10 (death freeze) is set by `Player_Die` only inside the bombs ≥ 1 branch and only while a spell card is active (g_Gui spell flags bit 0). The port reproduces the freeze itself (gameplay identical: Ar02 and Ro168 have 0 differing rows over the whole session once the column includes it) but keeps it in `Player::freeze_bullets`, so GameFlags (trace `flags`) never has 0x400. Second difference found in the same code: the port sets the freeze **outside** the bombs ≥ 1 branch, so a 0-bomb death during a card freezes in the port only (88234 row 55208: bombs 0, card active, port 0x3840c / original 0x3800c) | 09 §6.1 (bombs ≥ 1 branch only, card test without boss, flags column), 11 §2.1 bit 10 row | CODE BUG H |
| 1b | Dn14 next diff 5261 (bullets 15/16), 53373 47585 (score) | Reimu LS (`Bomb_ReimuLS`): orb i's impact switches its two T = 0 areas off, adds the impact C circle at the **player** and the D circle at the orb — and then the orb's area-follow write runs in the same frame through the old pointers. The new C circle took the just-freed slot (first inactive of the half), so it is moved to the **orb** position (orig scan: cancel areas 192–201 centred on the orbs). The port leaves it at the player, so bullets 985 / 1064 (stage 6A, far from the player) are cancelled frames later. Also the growth rates are the exe's 0x40888889 = 64/15 and 0x41088889 = 128/15; the spec's "4.267 / 8.533" were rounded and the port used those decimals (radius 110.937 vs 110.933 after 11 frames) | 14 §2 (exact-constant note), §2.1 / §2.2 (64/15, 128/15), §2.2 impact follow-write QUIRK | CODE BUG I |
| 2 | 80406 2822 (rng_ctr −4, stage 1 frame 2633) | The 4-spark death spawn (`Effect_Spawn(e+0x3311 + 4, pos, 4)`, 07 §10) finds 1 free ring slot in the original, 0 in the port: ring slot 273 (type 51 background particle) is freed by the view-cone cull at age 131 in the original and survives one more frame in the port. Particle world position and camera are bit-identical in both; the difference is the facing. The cull reads the **stored** normalised facing +0x63B8 written by `Stage_Calc`, and every normalise in this path is D3DX's `D3DXVec3Normalize`, which on the reference machine dispatches to the SSE variant 0x48EFB0: s = (x²+y²)+z², r = `rsqrtss`(s), m = (0.5·r)·(3 − s·r·r), v·m. For facing (0, 500, 460) it gives (0, 0x3F3C65F9, 0x3F2D5393) (read back from 0x4EA3E8); the port's exact v/|v| is 1 ulp higher in y and z, the dot rounds to ≥ 0.94 and the particle is not freed. Dot: `Vec3_Dot`, 24-bit x87, (z·z' + y·y') + x·x' | 05 §10 "Vector normalise" (new), 06 §3 row 0x63B8, 13 §2.6a cull paragraph | CODE BUG J |

Confirmation (scratch port with D–G + H + I + J, all 22 pilot sessions, re/work/runs/q52/fixJ):
- 0 differing rows: Ar02, Ro168 (bit-10 group), plus Bu08, JS06, Nd115, Tt16, F128 as before.
- Moved: Dn14 5048 → 7339 (rng), 53373 47393 → 50932 (rng), 80406 2822 → 76752 (flags), 88234 55208 → 62194 (rng/score).
- Unchanged first diffs (not analysed): Bi01 7416, 50715 3680, 51249 8330, 51842 2891 (score), 51852 4717 (RE-Q49 bug,
  not in the scratch copy), 52089 18346, 52104 22171, 52220 16139, 53084 7021, 80412 4011, 82309 6870.

Tooling: re/probe/session.inc gained TH08_SCANEFF / TH08_SCANEFF_ONLY / TH08_SCANEFFSLOT (scan.txt `R` and `P`/`D`
lines, only inside the TH08_SCAN poll window). Note: the poll index of a gameplay frame differs between re-records of
the same replay (loader race, ±10 polls), so scan windows should be wide.

## CODE BUG list
H. `player.rs` `Player::player_die` (+ trace): (a) the death freeze must be GameFlags bit 10 (or at least be reported in
   the flags word the session/demo trace samples: `demosync::sample` row 21) — today `freeze_bullets` is a separate
   bool and every spell-card death row differs in `flags`; (b) set it only inside the `bombs >= 1` branch (after the
   ring), not for a 0-bomb "Miss"; the doubling test is card-active only (`env.spell`), no boss test (09 §6.1, 11 §2.1).
I. `bomb.rs` Reimu bombs: (a) constants 4.267 → 64.0/15.0 (0x40888889) and 8.533 → 128.0/15.0 (0x41088889) in
   `orb_explode` and the LS blasts; (b) `bomb_reimu_ls` orb loop: on the impact frame, after `orb_explode`, still write
   the orb position into the orb's **old** C and D area slots (the indices held before `areas_off`), whatever they now
   contain (14 §2.2 quirk). Not for `bomb_reimu` (2.1).
J. `effect.rs` (types 19 / 51 / 63 cull, orbit 13) + `stage.rs` (`facing_norm`): implement the D3DX SSE normalise of
   05 §10 (native `rsqrtss` on x86_64) and use it for `Stage_Calc`'s +0x63B8 and for the particle direction; the cull
   must use the stored `facing_norm`, not `norm(view.facing)`; dot in the order (z + y) + x. Scratch version in
   q52_scratch.diff (`effect::d3dx_norm`, `View::facing_norm`). Check `EffUpd_Orbit13` and any other port
   normalise that mirrors a call through 0x476E9F.
