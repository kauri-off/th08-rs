# specs/data — runtime reference data of the original

## Attract demo traces `demo0_trace.txt` … `demo3_trace.txt`

One row per logic frame of each attract demo, from gameplay frame 1 until the demo returns to the title, measured
on the original under wine (VERIFIED, 0 differences across three runs with different boot seeds). The behaviour
these traces confirm is in the numbered specs (11 §1.2–1.5, §3, §4; 15 §3.3).

| file | replay | stage | shot | frames | spell rows | fast-forward rows |
|---|---|---|---|---|---|---|
| demo0_trace.txt | demorpy0.rpy | 5 | 0 | 6120 | 0 | 0 |
| demo1_trace.txt | demorpy1.rpy | 4A | 1 | 4920 | 0 | 0 |
| demo2_trace.txt | demorpy2.rpy | 3 | 3 | 5040 | 654 | 207 |
| demo3_trace.txt | demorpy3.rpy | 2 | 2 | 7020 | 567 | 26 |

### Format
- Text, space-separated.
- Three `#` header lines:
  1. demo, replay file and stage;
  2. the RNG state just **before** frame 1, and the number of loading frames;
  3. the column names.
- Each row is the state **after** that frame's calc chain. The sample point is the entry of `Sound_Update`, which
  Frame_Run calls right after `FuncChain_RunCalc`. When the chain is restarted (return 6, replay fast-forward), the
  sample is taken at the start of each new pass instead.
- Floats are given as the hex of their IEEE-754 bits, so they compare bit-exactly.

| col | name | source | format |
|---|---|---|---|
| 1 | frame | GM+0x3DDC0 gameplay frame counter | dec |
| 2 | demo_frame | GM+0x3DBB8 demo frame counter (+1 every logic frame; the key column) | dec |
| 3 | input | 0x164D52C game input word for this frame (replay-substituted) | hex u16 |
| 4 | rng | g_Rng state 0x164D520 | hex u16 |
| 5 | rng_ctr | Next16 calls since the state in header line 2 (cumulative) | dec |
| 6, 7 | px, py | player pos 0x17D61AC / 0x17D61B0 | f32 bits |
| 8 | pstate | player state byte 0x17D5EF8 (0 normal, 1 respawning, 2 dying, 3 invulnerable; 09 §2) | dec |
| 9 | score | stats+0x08 (score ÷ 10) | dec |
| 10 | graze | stats+0x0C graze total | dec |
| 11 | points | stats+0x30 point items total | dec |
| 12–14 | lives, bombs, power | stats+0x74 / +0x80 / +0x98 | f32 bits |
| 15 | gauge | stats+0x22 human/youkai gauge | dec i16 |
| 16 | time | stats+0x3C time this stage | dec |
| 17 | bullets | live bullet count 0x160F3C8 (from the last BulletMgr calc) | dec |
| 18 | enemies | number of enemy slots with alive bit (e+0x3324 bit 0), 480 slots at 0x577F20+0x53D0 | dec |
| 19 | rank | GM+0x3DE2C | dec |
| 20 | spell | g_Gui (0x4EA670) bit 0, spell card active | 0/1 |
| 21 | boss | bitmask of non-null boss pointers 0xF54CC0[0..3] | hex |
| 22 | flags | GM flags 0x164D0B4 | hex |
| 23 | ff | index of the calc-chain pass inside one Frame_Run. 0 = normal; 1, 2 = extra passes while the replay fast-forwards (input bit 0x100, 15 §3.3; up to 3 logic frames per Frame_Run) | dec |


### Using them for a regression test
1. Load `demorpyN.rpy` into the port as an attract demo:
   - shot, difficulty and stage come from the replay;
   - GM+0x3DBAA = shot % 2;
   - demo flag set.
2. Run the stage setup, then check that the RNG state equals header line 2.
3. Step one logic frame per row. Compare at least `rng`, `rng_ctr`, `px`, `py`, `score`, `lives`, `bombs`, `power`.
   The first mismatching frame localises the divergence.
   - `bullets` and `enemies` help to tell an RNG bug from a spawn bug;
   - `input` checks the replay reader, including the 1-frame latency and the key-repeat emulation.


## Reference screenshots `ref/`

Back-buffer captures (640×480 X8R8G8B8 → PNG) taken just before `Present`, windowed, default config, effect
level 2, music/SE off. wined3d: per-pixel filtering can differ slightly from a Windows driver; geometry, order,
colours and fades are reliable.

### Spell practice, card 54 (`夢符「二重結界」`, Easy, Reimu, stage 4A, shot 0 = Reimu & Yukari)
- Reached through the real menus: Spell Practice → stage row 3 → first card.
  - To unlock the card, story-attempts[54] (0x160F69C + 54·0x22C) was poked to 1.
  - The player stays at the spawn position and holds Shot from the start.
- Frame numbers are **gameplay frames (GM+0x3DDC0)** of that run. The boss's ECL sub28 timeline starts the card at
  gameplay frame ≈ 0 of the card. Native 3 runs at sub time 90.
- The RNG was not re-seeded in spell practice, so bullet positions vary between runs. The overall timeline and the
  pictures are stable: a second run was visually identical.

| file | frame | shows |
|---|---|---|
| spell54_ring_f059.png | 59 | spell-declare ring (large wavy ring) around the boss position, before the boss appears |
| spell54_ring_f074.png | 74 | ring collapsing into the starburst |
| spell54_burst_f089.png | 89 | burst at the boss, boss arriving at (192, 208) |
| spell54_cutin_f134.png | 134 | portrait cut-in, spell background, card name |
| spell54_boss_under_fire_f209.png | 209 | boss under fire: player shots hitting Reimu, score rising, boss life bar |
| spell54_natives3_mask_f269.png | 269 | **natives-3 mask**: square frames (effect 9/10 rings inscribed squares) + mirrored spell background, first bullets |
| spell54_natives3_mask_f389.png | 389 | natives-3 mask with bullets crossing the barrier; boss under fire |
| spell54_natives3_mask_f479.png | 479 | same, later, dense |
| spell54_capture_flash_f539.png | 539 | card captured: white flash, bonus text |

### Ending fade (`end00a.end`, bad ending, Reimu & Yukari)
- Reached from the same spell-practice game by poking Supervisor next-scene 0x17CE8B4 = 9 at scene-2 present 300.
  `Ending_Init` chose `end00a.end`, which matches 16 §1.2 (shot 0, bad ending).
- The background is `end00.jpg`: the downscaled capture matches it with a mean abs diff of 0.6/255.
- Frame numbers = Presents since scene 9 was entered (`s`).
- Script: `@2 30000` (white overlay with alpha 255 − t·255/30000), `@w 120 120`, then `@2 180` (fade from white over
  180 frames).

| file | s | pixel (20,20) RGB | mean |
|---|---|---|---|
| ending_fade_s0115.png | 115 | 255,255,255 | 255.0 |
| ending_fade_s0125.png | 125 | 254,253,253 | 254.0 |
| ending_fade_s0155.png | 155 | 243,236,237 | 243.5 |
| ending_fade_s0185.png | 185 | 231,217,220 | 232.7 |
| ending_fade_s0215.png | 215 | 220,200,204 | 222.1 |
| ending_fade_s0245.png | 245 | 208,181,187 | 211.4 |
| ending_fade_s0275.png | 275 | 197,164,171 | 200.8 |
| ending_fade_s0305.png | 305 | 186,147,156 | 191.0 |
| ending_fade_s0450.png | 450 | 186,147,156 | 190.9 (fade finished) |


Other folders: `drawlog/` (draw-log format and reference logs), `sessions/` (recorded sessions, README there),
`qNN/` (evidence for the RE-QNN / IMPLnn answers, see `specs/changelog/`), `spell_tables.md` (spell-practice text
tables).
