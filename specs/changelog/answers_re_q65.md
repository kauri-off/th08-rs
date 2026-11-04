# RE-Q65 — draw-log differences on the 20 ref_b2 draw-log recordings (2026-10-30 / 31)

Runs: re/work/runs/q65 (CHECKPOINT.md). Scratch port = q64 portcopy (all RE-Q64
fixes) + RE-Q65 fixes A–N behind `Q65NO<letter>` env switches (re/work/runs/q65/portcopy, target/re65). Recordings: the
ref_b2 draw-log set (re/work/runs/batch2/drawlog.txt). Before = master 395bac0 (target/dl_b2); run `d` = q64 portcopy + A–G
(classification run, all 20); after = run `g` = A–N (bin_g), golden = `golden_g`. The work was interrupted by a poweroff
after run `d` had started; d, e, f were rerun, then e/f were replaced by the final run g.

## Changed spec sections
| section | change | evidence |
|---|---|---|
| 07 §10.6 (trail draw) | QUIRK: the trail sprite copies overwrite the main VM's draw pos (+0x208…, z 0.3) and rotation; only scale and colour are restored, so the main VM is drawn at the last drawn copy's pos, z 0.3 | `EnemyMgr_DrawLayers` 0x42E140; udMu09 present 11473 (stg4aenm boss sprite lags one trail step) |
| 07 §11.1 step 5 (`Spell_Calc` start ring) | the shrink restart reads the boss's **live** timeout e+0x3378: after the timeout (−1) it writes duration −101 → slot inactive, the ring stays at radius 8, it never restarts at 256 | 0x416B90; 54576 present 11756 (port: ring radius 256 again) |
| 09 §6.2 (respawn) | respawn sprite script = 0 if (shot < 4 and focus +3 = 0) or shot even, else 5 (QUIRK: focused teams 0 / 2 respawn with the human script) | `Player_DyingUpdate` 0x44CBF0; 54194 present 29484, udL917 12629 |
| 09 §9.3 (gauge-max aura) | exact rule: spawn / move / free of the aura (type 25, slot 8, +0xE2B24) is inside `Player_Move`, so it is neither moved nor freed in states 1 / 2 | 0x44AEC0; udL917 presents 6662–6680 |
| 10 §4 step 2 | every reader of the swapped face pointer follows the 6B / Ex swap, incl. the boss spell cut-in | `Msg_Start` 0x43396D, 0x415F00 |
| 12 §1 (menu after a game) | `MainMenu_OnRegister`: previous scene 2 → `Loading_BeginCapture` (capture after the next Present) **and** `Loading_StartDarken(0)`; frame 1 draws no surface 8 but the darken quad at alpha 0 | `MainMenu_OnRegister` 0x470A6C (`0x17CE8B8 == 2` → 0x44886F + 0x4489CA); udKa45 14622, udRr19 10918, udMu09 19588 |
| 12 §5.1 (+0x24) | HUD boss timer frozen while timeout < 0 (threshold / death / timeout set −1); a port must not write (−1 − timer)/60 | `EnemyMgr_Calc` test `−1 < e+0x3378` before 0x42B930; 54576 4702, 54194 5148 |
| 12 §5.8 table row k = 1 | the boss cut-in file is the pointer g_Gui+0x262C (0x4ECC9C), which `Msg_Start(0)` swaps in 6B / Ex → after the Ex pre-boss dialogue the cut-in is `face_st08`, not `face_st08m` | `Gui_ShowBossSpellName` 0x415F00; 51394 16136, udCz17 16183, 51550 14163, 53044 22355 |
| 12 §5.8 step 4 (bonus line) | bonus and "capturable" are read live from g_Gui at draw time, also after `Spell_End` → a timeout ended in the same frame by op 123 shows "0" | 0x42BC50 / 0x4178C0; 53044 9413 |
| 13 §2.6c (callback 0x428310) | while paused / game over only the D updates are skipped; `s += D` always runs | 0x428310; udDe13 1645, udCr33 3360 |
| 13 §5 (spell background) | values of table 0x4C7144: 0xD8–0xDD → eff09sk / ym / al / rm / yy / yk (entries 9–14 after the stage table) | `EffectMgr_OnRegister` 0x4284B0, exe 0x4C74A4; udGu03 305 |
| 14 §2.17 (dissolve) | `Bomb_Dissolve` writes the ring fields (width +0x320 := 15 …), not the VM scale; the type-50 update copies VM scale.x (script 80: 128 → 15) → first frame width 124.8 | 0x40D100; udMu09 19319 |
| 06 §10.2 (tint lifetime) | prio-11 enemies and `BulletMgr_Draw` turn the tint off **only while bit 10**, `Spell_Draw` only inside its VM-7 block → on the frame after the last bomb draw the bomb tint covers the whole frame (bullets, HUD, text) | 0x42EB90, 0x432B50, 0x4178C0; udLe29 1902 (all quads 0xFFABABAB) |
| data/drawlog/FORMAT.md §3 (K) | `K screen ? full` after a playfield capture = the D3DX surface load's internal back-buffer copy in the recording environment; comparators ignore it | `Anm_CaptureToTexture` 0x466F20, probe `h_CR`; udRr19 10826 / 10918, golden 10796 / 10918 |

Already in the specs (no change, port bug only):
- 07 §10.1 step 4 / §10.2: `Enemy_CheckTimeout` is called only while timeout ≥ 0 (cause A).
- 11 §4 step 1: a replay with no later stage writes scene := 7 directly (next_scene_after stays −1) → `Hud_StageTransition`
  still captures on the clear frame (udRr19 10918: `K screen capture.anm#0 32 16 384 448 128 0`) (cause C).
- 13 §2.6 type 50: the update sets segments := 32 every frame (54194 12531: port drew the dissolve ring with 64) (cause F).

## Causes (Q1), earliest first difference per recording
| cause | recordings / present | original | spec | port |
|---|---|---|---|---|
| A boss timer after timeout −1 | 54576 4702, 54194 5148 | HUD +0x24 untouched while timeout < 0 | 12 §5.1, 07 §10.1 | CODE BUG |
| B trail leaves main VM at last copy | udMu09 11473 | main VM drawn at last copy pos, z 0.3 | 07 §10.6 | CODE BUG |
| C replay-end clear-frame capture | udRr19 10918, udXu01 10520 | scene := 7 directly, capture on clear frame | 11 §4 | CODE BUG |
| D gauge-max aura frozen while dying | udL917 6663 | aura code only in `Player_Move` | 09 §9.3 | CODE BUG |
| E bonus line after same-frame timeout + end | 53044 9413 | live g_Gui fields → "0" | 12 §5.8 | CODE BUG |
| F dissolve ring 32 segments | 54194 12531 | type-50 update segments := 32 | 13 §2.6 | CODE BUG |
| G dissolve ring width from VM scale | udMu09 19319 | ring width field, VM scale untouched | 14 §2.17 | CODE BUG |
| H menu load after a game | udMu09 19588, udKa45 14622, udRr19 10918 (every recording that returns to the menu) | capture + darken | 12 §1 | CODE BUG (not scratch-fixed) |
| I Ex boss cut-in file | 51394 16136, udCz17 16183, 51550 14163, 53044 22355 | swapped pointer 0x4ECC9C | 12 §5.8, 10 §4 | CODE BUG |
| J respawn sprite script | 54194 29484, udL917 12629 | team test quirk | 09 §6.2 | CODE BUG |
| K start ring restart after timeout | 54576 11756 | live e+0x3378 − 100 = −101 | 07 §11.1 | CODE BUG |
| L spell-practice eff09xx background | udGu03 305 | table 0x4C7144 | 13 §5 | CODE BUG |
| M bomb tint on the frame after the last bomb draw | udLe29 1902 | conditional tint-offs | 06 §10.2 | CODE BUG |
| N list-B particles on game-over frame | udDe13 1645, udCr33 3360 | `s += D` always | 13 §2.6c | CODE BUG |

Classified, no change:
| recording / present | what | cause |
|---|---|---|
| udXu01 3285, 50404 1866, udEL01 2500 / 2535 | etama#1 Q uv (16, 96) pos 0.8–3.5 px for 1 frame | list-B particle player-push flip, 13 §2.6c precision (skip) |
| udCz17 2200, 51394 4222 | etama#1 R (sprites 198 / 202, type-4/5 sparks) one snapped edge 1 px off for 1 frame | x.5 tie at the Draw2DSnap rounding; init/update/draw order matches 0x425D70 / 0x425E60 / 0x427F00 / 0x462FF0 → f32 precision (skip) |
| udL917 6816 | sliding-text glyph 15 vs 14 px | snap tie (precision), mis-paired by drawdiff's greedy pass 2 (tool fix below) |
| 50996 / 78538 215 | loading gauge strip 1 px for 1 frame | loader race, 12 §5.9 (confirmed) |
| udMu09 6100, udRr19 4162, 53044 2092, udL917 6663, 51550 6681 | spell-bg rotation, boss dir anim, ribbon shake, static boss popup at freeze start | fixed by RE-Q64 (Q1a, Q1c, Q2b, Q3b) |

Next first differences after run g (not investigated):
- udXu01 32450: at the start of a dialogue (textbox opens) the original keeps the boss bonus line (10 glyphs, y 47.5)
  for ~78 more frames; the port hides it and slides in a 6-glyph line (12 §5.8 / 10 §4 — likely a port reset at `Msg_Start`).
- 50404 25601: FPS counter digits (383, 447) differ (environment timing; check against the FPS text spec before treating it as a bug).
- 78538 247, 50996 252: loading gauge glyphs at alpha 06 — probably the loader race family (12 §5.9).
- 51394 20885: etama#2 colour. udGu03 5240: stgenm_yk.anm colour.
- 54576 33380, 54194 32884: ascii.anm.
- udCz17 30106, udL917 27747, 51550 26156, 53044 34279, udAy01 26378: etama#1 pos 1–2 px (probably precision).
- udEL01 2535: precision.
- udRr19 10918, udKa45 14622, udMu09 19588, udLe29 1936, udCr33 3362, udDe13 1647: H (menu after a game, not scratch-fixed).

## Q2 — spell-practice recordings with the RE-Q64 fixes (run d = q64 portcopy + A–G)
| recording | master | run d (RE-Q64 + A–G) | cause | after (run g) |
|---|---|---|---|---|
| udCr33 | 305 | 3360 | N | 3362 (menu H) |
| udDe13 | 305 | 1645 | N | 1647 (menu H) |
| udGu03 | 305 | 305 (eff04a vs eff09yk) | L | 5240 (stgenm_yk colour) |
| udLe29 | 306 | 1902 | M | 1936 (menu H) |
RE-Q64's spell-background / HUD fixes remove all 305/306 differences except udGu03, which needs L.

## Tooling (scratch drawlog.rs / drawdiff, re/work/runs/q65/portcopy)
- Missing `K screen ? full` ignored (FORMAT.md §3) — recommended for src/drawlog.rs.
- Pass 2 (nearest pairs) assigns candidate pairs globally by ascending cost instead of greedily in port order (a far port
  quad can no longer take a ref quad whose own near port quad comes later) — recommended for src/drawlog.rs.
- `Q65SKIP=1` (RE only): pos-only changes of the list-B particle sprite (etama#1 [16,96,32,112], ≤ 3 px) and of any quad
  whose vertices all moved ≤ 1.01 px are not scored — used only to find the next real first difference.
- `Q65DBG=1`: prints pass-2 pairs.

## Effect (scratch draw-log compare; trace identical in every run)
Columns: first difference (present) before (master, target/dl_b2), in run d (q64 portcopy + A–G) and after (run g = A–N,
Q65SKIP); identical frames before, after without skip (run gn = bin_g re-compared by standalone drawdiff, so the frame
counts differ by ~18 per recording) and after with Q65SKIP. Trace (T=) is identical in every run.

| recording | first before | first d | first after (g) | identical before | identical after (no skip) | identical after (Q65SKIP) | trace |
|---|---|---|---|---|---|---|---|
| gensokyou_th8_udXu01 | 3285 | 32450 | 32450 | 94753/112389 | 109816/112406 | 110794/112389 | T= |
| silentselene_50404 | 1866 | 25601 | 25601 | 85672/111511 | 109330/111530 | 110022/111511 | T= |
| silentselene_54576 | 4702 | 11756 | 33380 | 80239/115562 | 108518/115577 | 109513/115562 | T= |
| silentselene_54194 | 5148 | 29484 | 32884 | 78689/109524 | 108046/109542 | 108884/109524 | T= |
| silentselene_78538 | 215 | 247 | 247 | 95031/123261 | 118197/123279 | 119400/123261 | T= |
| silentselene_50996 | 215 | 252 | 252 | 95419/134510 | 130143/134529 | 132682/134510 | T= |
| silentselene_51394 | 4222 | 16136 | 20885 | 26461/49775 | 42666/49792 | 46942/49775 | T= |
| gensokyou_th8_udCz17 | 2200 | 16183 | 30106 | 25390/47510 | 40392/47528 | 45985/47510 | T= |
| gensokyou_th8_udL917 | 6663 | 12629 | 27747 | 22651/44379 | 38321/44397 | 43095/44379 | T= |
| silentselene_51550 | 6681 | 14163 | 26156 | 22641/41822 | 37808/41840 | 41094/41822 | T= |
| silentselene_53044 | 2092 | 22355 | 34279 | 27768/77027 | 46282/77044 | 65694/77027 | T= |
| gensokyou_th8_udRr19 | 4162 | 10918 | 10918 | 10792/10930 | 10907/10948 | 10899/10930 | T= |
| gensokyou_th8_udKa45 | 7183 | 14622 | 14622 | 14257/14633 | 14619/14652 | 14602/14633 | T= |
| gensokyou_th8_udMu09 | 6100 | 19588 | 19588 | 9806/19601 | 19576/19618 | 19570/19601 | T= |
| gensokyou_th8_udLe29 | 306 | 1902 | 1936 | 1495/1947 | 1930/1966 | 1916/1947 | T= |
| gensokyou_th8_udCr33 | 305 | 3360 | 3362 | 336/3374 | 3361/3392 | 3343/3374 | T= |
| gensokyou_th8_udGu03 | 305 | 305 | 5240 | 287/5295 | 5275/5313 | 5262/5295 | T= |
| gensokyou_th8_udDe13 | 305 | 1645 | 1647 | 348/1659 | 1646/1677 | 1628/1659 | T= |
| gensokyou_th8_udAy01 | 4850 | 13302 | 26378 | 105996/136635 | 133385/136649 | 135058/136635 | T= |
| gensokyou_th8_udEL01 | 2500 | 2535 | 2535 | 7052/7446 | 7341/7465 | 7412/7446 | T= |
| **total** | | | | 805083/1168790 (68.9%) | 1087559/1169144 (93.0%) | 1133795/1168790 (97.0%) | |

Golden (sessions/golden, bin_g): 8009/8013 frames identical without skip (gn_golden.txt), 8013/8013 with Q65SKIP
(g_golden_skip.txt); the first divergence stays at present 4439 (same as RE-Q64, precision).
H (menu after a game) is not scratch-fixed: udRr19 10918, udKa45 14622, udMu09 19588, udLe29 1936, udCr33 3362,
udDe13 1647 now end on it.

## CODE BUG list
Reference implementation of each fix (except H): re/work/runs/q65/portcopy, look for `Q65NO<letter>`. Functions are named as in master src.

| # | src function | change | spec |
|---|---|---|---|
| A | `src/enemy.rs` `EnemyManager::check_timeout` (or its caller in the per-enemy calc) | do not call it (no HUD +0x24 write, no fire test) while `e.timeout < 0`; the HUD keeps its last value | 07 §10.1 step 4, 12 §5.1 |
| B | `src/enemy.rs` `draw_layer` (main VM after `draw_trail`) | when ≥ 1 sprite copy was drawn (F bit 3 clear), draw the main VM at the last drawn copy's pos (E[i].pos + offset + origin), z 0.3, with that copy's rotation if flag A bit 25; scale / colour restored as now | 07 §10.6 |
| C | `src/gamemgr.rs` `stage_end` (replay with no later stage) | request scene 7 directly; leave `next_scene_after` at −1 so `Hud_StageTransition` captures on the clear frame | 11 §4 step 1 |
| D | `src/player.rs` `frame` (gauge-max aura block) | run the aura spawn / follow / free only on frames where `Player_Move` ran (not in states 1 / 2) | 09 §9.3 |
| E | `src/gui.rs` calc (bonus / capturable copy) + `src/enemy.rs` `end_active_spell` | the bonus line reads the live g_Gui bonus +0xFC / spell flag bit 2 also after `Spell_End` (a same-frame timeout shows 0), not the copy taken while the card was `Some` | 12 §5.8 step 4, 07 §10.2 |
| F | `src/effect.rs` `update` (ring types) | type 50: `segments := 32` every update | 13 §2.6 type 50 |
| G | `src/bomb.rs` `dissolve_start` | write ring width 15, radius 8, start angle 0 (ring fields), not `vm.scale[0] = 15` | 14 §2.17 |
| H | `src/supervisor.rs` `menu_loading` (+ the menu draw of surface 8) | previous scene = game (2): start the loading VMs, request the post-present capture into surface 8 (no logo load) and create the loading darken overlay (`effect::loading_overlay`, ScreenEffect type 5, 60 frames, black, prio 1); frame 1 draws no surface 8 | 12 §1 |
| I | `src/gui.rs` `handle` `Req::BossShow` | use the dialogue's `face_slot[2]` (swapped by `Msg_Start(0)` in 6B / Ex) instead of the fixed `SPELL_FACE_SLOT` 18 | 12 §5.8 k = 1, 10 §4 |
| J | `src/player.rs` `dying_update` (respawn) | script = 0 if (shot < 4 && focus == 0) \|\| shot even, else 5 — not `sprite_base(focus)` | 09 §6.2 |
| K | `src/enemy.rs` `spell_step` step 1.5 | restart duration = live `boss.timeout` − 100 (not `s.duration`); a negative duration leaves the slot inactive | 07 §11.1 step 5 |
| L | `src/effect.rs` `bg_file` / `EffectManager::on_register` | spell practice with card id 0xD8–0xDD: load `eff09sk / ym / al / rm / yy / yk.anm` into slot 9 | 13 §5 |
| M | `src/bullet.rs` `draw`, `src/enemy.rs` `draw_layer` (high), `src/gui.rs` `draw` | tint off only while bullets are frozen (bit 10) — bullets after `draw_over_bullets`; Gui only inside its VM-7-visible block (drop the trailing unconditional `tint_off`) | 06 §10.2 |
| N | `src/effect.rs` `draw_background` (types 51 / 63) | apply `s += disp` also while paused / game over; only the pull / push updates are skipped | 13 §2.6c |
| T1 | `src/drawlog.rs` comparator | ignore a missing reference `K screen ? full` after a capture | data/drawlog/FORMAT.md §3 |
| T2 | `src/drawlog.rs` pass 2 | assign nearest pairs globally by ascending cost (tool quality, no spec) | — |
