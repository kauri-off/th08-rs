# Answers → numbered specs merge audit (RE-Q23)

One row per answer section (or table row group). Status: **there** = the numbered spec already states it;
**added** = written into the spec in this audit; **corrected** = the spec contradicted it (or a later answer) and was
fixed; **obsolete** = superseded by a later answer (the later one is what the spec states).

| answer § | topic | target spec § | status |
|---|---|---|---|
| phase3 impl2-1/2-7/2-3 seed | menus, logo, RNG (deferred) | 12 §2, 12 §1, 11 §1 | obsolete (answered in phase5) |
| phase3 impl2-2…2-6, 2-8 | tint, draw-skip, op 59 n=0, render state, frame reset, flag 0x20 | 05 §12.1–12.6 | there |
| phase3 impl3-1, 3-3 | mode-5 anchor FABS, texture caches | 05 §12.7, §12.8 | there |
| phase3 impl3-2 | text.anm script 33 overlay, byte 0x17D5EF8+5 = player mode | 06 §13, §2 step 1 | corrected (06 step 1 still said "owner not identified") |
| phase3 impl3-4…3-6 | ribbons, billboard scale, seams | 06 §13, 05 §12.4 | there |
| phase3 footnote | op 111 raw ints | 07 §7 | there |
| phase5 impl2 (menu, RNG, logo) | title01 scripts, RNG Next16/seed, th08logo | 12 §2.1–2.2, 11 §1, 12 §1 | there |
| phase5 impl5 mirroring | mode-2 mirror cancellation, op 64/66/67 | 07 §8 | there |
| phase5 impl5 enemy draw | layers, prio 8/11, VM positions | 07 §10.6 | there |
| phase5 impl5 section request | op 147 = 0x93 | 06 §3/§4.1 | there |
| phase5 impl5 random direction | op 67/169/178 branches, old-angle QUIRK | 07 §7, §8.3 | there |
| phase5 impl5 timeline stall | net-zero clock | 07 §3 | there |
| phase5 impl5 interrupts | no arg copy, main context, depth += 1 even without push, return at depth < 0 | 07 §5.4, §5.5 | added |
| phase5 impl5 rank start | table 0x4C7880 | 11 §3.2 | there |
| phase5 impl5 death sub consumed | 2CEE := −1 by all users | 07 §10.3, §10.6 | there |
| phase5 impl5 spawn position | (192, 384) | 09 §6.2 | there |
| phase5 impl6 op 111 | variable-capable args, mask bits, raw ints | 07 §7 | there |
| phase5 impl6 bullet/item/laser draw | z 0.05/0.15/0.06, rotation only with tag | 08 §3, §6, §8.6 | there |
| phase5 impl6 graze/hit states | graze not in player states 1/2; hit returns 1 in any state | 08 §4.1, 09 §6.1 | added (graze states); hit: there |
| phase5 impl6 laser test cadence, cancel items, guard, cancel modes | | 08 §6, §7 | there |
| phase5 impl6 drop items mode | m = 1 only when a bomb area hit the enemy on the killing call | 07 §10.1 step 6, §10.4 | there (impl18 correction) |
| phase5 impl6 point item / converted point | | 08 §8.5 | there |
| phase5 impl6 ex cmd timers, natives 12/14, item y < −8 | | 08 §5, §7, §8.6 | there |
| phase5 impl6 rank mechanism, starting values | | 11 §3.2 | there |
| phase5 impl6 time-orb timers | shot timer, gauge lock 0x18B89D4 | 08 §8 | there |
| phase5 impl6 bomb checksum | AddBombs vs SetBombs | 09 §9.4 | there |
| phase5 impl7 bombs, options, Alice laser, shot rotation/tint/marker | | 14 §1–5 | there |
| phase5 impl7 contact /0.7, /1.5, −10 life | | 07 §10.1 | there |
| phase5 impl7 homing targets | +0xE2AB0 never written | 09 §1.1, §2.1 | there |
| phase5 impl7 bomb damage in cards | | 07 §10.1 | there |
| phase5 impl7 deathbomb counter, LS index, lives | | 09 §7, 11 §3.2 | there |
| phase5 impl7 input bits | full keyboard map 0x43D970 | 12 header | added |
| phase5 impl8 midboss removal, KillAll | | 07 §10.3, §10.6, op 95 | there |
| phase5 impl8 HUD +0x2C spawn block | | 07 §3 | there |
| phase5 impl8 stage-clear rank sprite, ename plate | | 12 §5.6/§5.7 | there |
| phase5 impl8 capture condition and reward | time reward formula (D, thr = D − D/7, r) | 07 §11.1 | added (the spec only referred to the answer) |
| phase5 impl8 continue prompt | default "No"; "Yes" in Extra/spell practice = retry | 11 §4.2, 12 §4 | there (default "Yes" when an SP card is not captured: 12 §4, phase5 correction) |
| phase5 impl8 over-80% lines, plates, overlap fade, shot hold | | 12 §5.6/§5.7, 13, 14 §4 | there |
| phase5 corrections list | | as listed | there |
| phase6 §1 | ending, SP details, formats | 16 §1–3, §6, 15 | there |
| phase6 §2 | spell flag bits 10 / 11 / 6, natives 22/23, op 164/184 | 07 §11.1, §7, §12 | there |
| phase6 §3 | large popup glyphs and draw order | 13 §4 | there |
| phase6 §4 | corrections (11 §6 record score at end, RNG save draws, LoadScoreData flush, 07 ops, 15 entry 214, PLST 7 blocks) | 11, 07, 13, 15 | there |
| phase6 §5 | open items (no-writer globals) | 15 §3.2 (fps flag), 11 §2 (0x164D0B2 demo use), 15 §1.1 (+0x26 read by the joystick poll) | there (resolved later) |
| phase7 intro | anti-tamper Supervisor fill | 15 §2.2, 11 §4, 09 §6.1 | there |
| phase7 §1 | fps flag 0x17CEA90 always 0 | 15 §3.2 | there |
| phase7 §2 | Supervisor+0x1A4 bits | 15 §2.2, §3.2 | there |
| phase7 §3 | byte 0x164D0B2 | 11 §2, 12 §2.2 | there |
| phase7 §4 | CLRD+0x20 | 16 §1.2 | there |
| phase7 §5 | joystick mapping, +0x26 shot-also-focuses counter, direction thresholds | 15 §1.1 | added |
| phase7 §6 | frame skip, fps meter, screenshots | 00 §2, 15 §1.1, §2.11, §3.3 | there |
| phase7 §7 | ending fade quad | 16 §1.5 | there |
| phase7 §8 | natives 3/6/20 intent | 07 §12 (draw steps, from impl9 §6) | there |
| phase8 §1 | x87 CW 0x007F main / 0x027F loader, f32 math | 05 §10, 11 §1.2 | there |
| phase8 §2 | demo traces: format, facts (seed + 2 draws, 81 loading frames, no title RNG, determinism, FF passes) | 11 §1.3, §3, 15 §3.3; format → specs/data/README.md | there (facts); added (format moved to data/README.md) |
| phase8 §3 | reference screenshots, ending fade check | 16 §1.3 (@2); list → specs/data/README.md | there; added (list) |
| phase8 §5 | open: native Windows CW, loader-thread chains | 11 §1.2 | there |
| impl9 P1 | hit spark type 3 only from damage areas, counter, RNG | 13 §2.6, §2.7, 09 §8 | there |
| impl9 P2 / §3 | ring UV mapping, width, spell ring, death ring | 13 §2.5, §2.7 | there |
| impl9 §4 | particles 19/51/63 init table and per-frame update | 13 §2.6a (new) | added |
| impl9 §5 | op 128 orbit effect 13 (spawn, enemy side, update, fade) | 13 §2.6b (new), 07 op 128 | added |
| impl9 §6 | natives 3/6/20: effects and stage mask callback steps | 07 §12 row, §12.1 (new) | added |
| impl9 §7 | effect file table 0x4C7480 / 0x4C7144 | 13 §5 | there |
| impl9 §8 | list B: effect level 1 draws nothing, billboard projection, helper 0x428310 | 13 §2.4, §2.6c (new) | added (helper/projection) |
| impl9 §9 | auras / invulnerability ring follow | 13 §2.6, 09 §1.1 | there |
| impl9 §10 | stage sparkles: points, spawn at all 12 points quirk | 06 §4 (Stage_Calc), §11.1 | corrected (spawn rule: spell practice, all 12, colour) |
| impl9 §11 | screen-shake consumers, offset zeroed at prio 14 | 13 §3 | there |
| impl9 §12 | popup colours per caller | 13 §4 | added |
| impl9 §13 | glyph 10 PowerUp | 13 §4 | there |
| impl9 §14 | Popup_NumberBig callers, large popup callers/colours/scale | 13 §4 | added (caller table) |
| impl9 §15–16 | title description VMs, option value VMs | 12 §2.2, §2.7 | there |
| impl9 §17 | unlock rules | 16 §4, 15 §2.4, 16 §6 | there |
| impl9 §18 | boot logo hold, missing cfg | 12 §1, 15 §1.2 | there |
| impl10 §1 | large popups (no motion, life 90, static start age 88, x origin, scale), boss "+0" popup | 13 §4, 12 §5.4 | there |
| impl10 §2 | `Enemy_KillChildren` chain-kill / boss / single-kill paths, orb counts, ÷3 quirk, callers | 07 §10.3a (new) | added (spec had only the call sites) |
| impl10 §3 | sparkle point bound (≤ 15) | 06 §11.1 | there |
| impl10 §4 | native 12 chain part, flags B bit 7 pause | 07 §4.4, §12, 08 §7 | there |
| impl10 §5 | practice card id, effect file, B34 writers, boss name plate table | 13 §5, 12 §5.2 | added (plate table) |
| impl10 §6 | `Msg_Start` entry-0 hooks | 10 §4 | there |
| impl10 §7 | last spell bit 5, op 176, Spell_End bits 7–8 | 07 op 176, §11.1 | added (bits 7–8 at Spell_End) |
| impl10 §8 | explosion hit filter | 09 §4.3 | there |
| impl10 §9 | Alice laser damage areas, AddDamageRect | 09 §8, 14 §4 | there |
| impl11 §1 | bomb tint lifetime, flag 0x4EA49C | 06 §10.2, 14 §1 | there |
| impl11 §2–5 | +0x3D per area, Reimu orbs, Reimu LS start, Sakuya knife one-shot | 14 §2.1–2.7 | there |
| impl11 §6–8 | Alice ring colours/alpha, plain ring before t 40, Remilia rings | 14 §2.3–2.4, §2.9 | there |
| impl11 §9 | Remilia team bats, Yuyuko options | 14 §3 table | there |
| impl11 §10–11 | Sakuya LS window, bomb effect lifetimes | 14 §2.7, §2.11–2.12, 13 §2.6 | there |
| impl11 §12 | Yukari aimed pointer persistence | 14 §3.1 | there (refined by impl18/21) |
| impl11 §13 | spell-name plate and cut-ins | 12 §5.8 | there |
| impl11 §14 | dissolve spell trigger and end flash | 14 §5, 09 §7 | there |
| impl11 §15 | death ring | 13 §2.7 (bugs1 B2) | there |
| impl12 §1–4 | KillChildren path choice, +0x3380 writers, boss gate, c vs ÷3 order | 07 §10.3a | added |
| impl12 §5 | GameFlags bit 13 whole-playfield box | 07 op 176 | there |
| impl12 §6 | native 22 details (face −1, VM +0x2374, later Spell_End no-op) | 07 §11.1 | added |
| impl12 §7 | CATK attempts/captures/timeouts | 15 §2.5, 07 §10.2 | there |
| impl12 §8 | SP effect file table | 13 §5 | there |
| impl12 §9 | full-repaint counter 0x17CE8CC writers | 12 §5.3 | added (spec pointed to the answer) |
| impl13 §1 | StageStart setup order (gauge VMs after Player_Load) | 11 §3, 12 §5.9 | there |
| impl13 §2 | full-exit / fresh-start flags, interrupt 99; retry counters | 11 §7, 12 §5.9; 11 §3.2 | there; added (retry counters, INFERRED) |
| impl13 §3, extra | gauge limits in the menu, extra VMs, G1/G2 positions | 12 §5.9 | there |
| impl13 §4 | fog model | 06 §8 | there |
| phase5 impl6 rank deltas | list of RankAdd/RankSub deltas | 11 §3.2 | added (spec pointed to the answer) |
| impl14 §1 | op-83 child effect 38, 10 draws every 2nd frame | 07 §10.1 step 3, 13 §2.6 | there |
| impl14 §2 | bullet counter semantics | 08 §4.3a | there |
| impl14 §3 | kill draw order (sparks, drop sparks, death sparks, time orb) | 07 §10.3, 13 §2.7 | there |
| impl14 §4 | death gauge ±200 by focus byte +3 | 07 §10.3 | there |
| impl14 §5 | options leaving state 3 (t > 16, Alice) | 14 §3 | there |
| impl14 §6 | replay stage load score / cfg copy | 11 §3.4, 15 §3.7 | there |
| impl14 §7 | child aura interrupt | 07 §4.3, op 90–92 | there |
| impl14 §8 | attract demo end fade, first demo index | 11 §4 | there |
| impl14 §9 | PLST counters, play time | 15 §2.9 | there |
| impl14 §10–11 | VRSM constants; flags 15–17 | 15 §2.10, 11 §2.1 | there |
| impl15 Q9 | type 51 particles: spawn source, 26 draws per effect, init expression order, spawn-frame order, colour2 | 13 §2.6, §2.6a | added (expression order); rest there |
| impl15 Q6 | loading darken overlay (type 5) | 13 §3 | there |
| impl15 Q7 | gauge readout timing | 12 §5.9 | there |
| impl15 Q1–Q2 | boot presents, loading VMs draw | 12 §1 | there |
| impl15 Q8 | HUD z / zwrite | 12 §5.3 | there |
| impl15 Q10 | panel tiles over the playfield edge | 12 §5.3 (repaint counter) | there |
| impl15 Q11 | stgNtxt default render state | 12 §5.1 | added |
| impl15 Q12 | menu screen-entry rule | 12 §2 | there |
| impl15 Q3–Q5 | menu lists pointers; music-room exit capture 0x44886F | 12 §2.5, §2.8, 16 §2, §5.3, 12 §1 | there |
| phase5 impl5 op 66 mirroring | op 66 (and 69, dur ≥ 1) negate delta.x when mirrored; 67/178 do not | 07 §7 op 66, §8.3 | obsolete (corrected by impl16 §8; the spec states the impl16 rule) |
| impl16 §1–3 | shot damage: uncapped return, accumulator += min(shot, 50), orb loop per hitting bullet | 09 §4.5, 07 §4.3 | there |
| impl16 §4–5 | drop sparks table, every-third counter start | 07 §10.4 | there |
| impl16 §6–7 | KillChildren step 5, +0x3380 | 07 §10.3a | there (added this audit) |
| impl16 §8 | op 66 mirror | 07 op 66 | there |
| impl16 §9 | two time orbs collected, shot timer example | 08 §8.4 | there |
| impl17 §2 | loading frame count (31/81 + upload floor), GameMgr_Calc z clear | 11 §3.1, §4 | there |
| impl17 §5.1–5.3 | stg4abg vertices, night clock timing, replay fps text | 06 §11, 12 §5.1, 15 §3.3 | there |
| impl17 Q1 | spell tables, LastWord_ComputeUnlocks, FLSP | 15 §2.8, 16 §6, data/spell_tables.md | there |
| impl17 Q3 | menu interrupt delivery | 12 §2.1–2.4, 16 §6.1 | there |
| impl17 Q4 | return from result viewer / music room, description VM timing | 12 §1, §2.2, 16 §2.4 | there |
| impl17 Q6 | text rasterizer layout, font substitution, 取得率 heading | 10 §6, 16 §6.4 | there |
| impl17 Q7 | fading description, card-name VMs | 12 §2.2, §2.8, 16 §6.5 | there |
| impl18 Q1–Q3 | hitByBomb rule, hit flash, draw-list prepend, aimed enemy ordering, previous vx, gauge while bombing, menu background/batch order | 07 §10.1, 14 §3.1, 09 §3, §7, 12 §2.2 | there |
| impl19 Q1–Q2, Q4 | SetPower clamp, screen-check box, loading floor | 08 §8.5, 07 §10.1, 11 §3 | there |
| impl19 Q3 (RE-Q19) | var 10096 chain count; child spawn-frame double run (wait n → spawn + n − 1); Next32 = hi << 16 \| lo | 07 §6, §10.5, 11 §1.1 | added (double-run consequence); rest there |
| impl20 Q1 | var 10098 a + b + c ≥ d | 07 §6, 11 §5 | there |
| impl20 Q2 | death modes never clear the boss table; HUD byte by mode | 07 §10.3 | there |
| impl20 Q3 | death-latch single clear point | 07 §10.3 | there |
| impl20 Q4 | screen-check box | 07 §10.1 | there |
| impl20 Q5 | row 6264 hit-spark count | 09 §8, 13 §2.6 (spark gate) | there |
| impl20 notes | op 36 float var id; Msg_Run prio; wait_boss; freeze byte writers | 07 §7, 12 §5, 07 §3, 11 §2 | there |
| impl21 Q1 | var 10096 walk, link position, parent = spawner | 07 §6, §10.5 | there |
| impl21 Q2 | Yukari branch order | 14 §3.1 | there |
| impl21 Q3 | GameFlags bit 2, frame counter 0x164D2C8 | 11 §2, §2.1 | there |
| impl21 Q4 | cancel/collection rules, sticky +0xDBE, AddTime parity, human-sht attract values | 08 §4.2, §7, §8.4, 11 §5 | there |
| impl21 Q5–Q6 | pointers | 07 §10.5, 09 §8 | there |
| impl21 Q7 | trail ring | 07 §4.3, §10.1 | there |
| bugs1 B1 | Barrier hit effect: rotation save/restore, tag re-set | 09 §4.5, 05 §2, 14 §5 | there |
| bugs1 B2 | death ring lifecycle | 13 §2.7 | there |
| bugs2 C1–C3 | menu seam, character select plate, in-game visuals (drop sparks, gauge readout) | 05 §12.4, 12 §2.2, §2.4, §5.9, 13 §2.7 | there |
| boss_phases §1–2 | stage 1 boss script and sequence (data) | evidence only (07 §10.2–10.3 state the rules) | there |
| boss_phases §3 | transition rules, depth storage | 07 §5.4, §10.1–10.3 | there |
| re_q22 | option volume, key config, +0x6C return path | 15 §1.3, 12 §2.2, §2.7, 16 §6.1 | there |
