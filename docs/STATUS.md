# Status

## Done
- `src/archive.rs` — PBGZ th08.dat reader (header/table cipher, LZSS, edz payload wrapper). All 317 files byte-identical to DeepSeek's extraction. `cargo run --release --bin datdump <th08.dat> <out>`.
- `src/audio.rs` — thbgm.fmt parse + cpal stream, linear resampling to device rate, intro then loop to `+0x18` (loop start/end VERIFIED by specs/02_bgm.md). Device pick: pipewire → pulse → default (ALSA default is broken on this machine).
- `src/app.rs` — winit app, original frame timing (1/60 s deadline, late frames dropped), input bitmask (`src/input.rs`), plays th08_01.
- `src/gfx.rs` — wgpu sprite batcher; `capture()` for screenshots. `web/` — index.html + build.sh (wasm-bindgen --target web).
- Debug: `TH08_SHOT=<frame>` saves /tmp/th08_shot.png (offscreen 640x480 render) and exits. Use `RUST_LOG=warn,wgpu_hal=off` (a broken Vulkan layer spams errors).

- `src/anm.rs` — ANM chunks/sprites/scripts + THTX→RGBA (visually verified: player00 fmt5, eff01 fmt3). `cargo run --release --bin anmview <th08.dat> <x.anm> <outdir>` dumps textures with sprite rects.
- `src/task.rs` — task system (00_overview §3): calc/draw chains sorted by u16 priority (insert after equal), on-register/on-remove hooks, return codes 0–6 (`Ret`), runner → `Flow`. Generic over the context (`TaskCtx`), unit-tested.
- `src/game.rs` — `Ctx` (gfx, archive, audio, input, anm slots, tasks); one calc pass per executed frame, then the draw pass (`src/app.rs`).
- `src/supervisor.rs` — Supervisor (calc 0, draw 0/2/16; loads text.anm→0, nowloading.anm→2; polls input) registers Title (calc 4, draw 3; title00.png, title01.anm→20). `TH08_SLOT_TEST=<slot>` draws that slot's sprites clipped to the playfield.
- `src/anmmgr.rs` — 25 ANM slots: load/unload, cumulative sprite/script index per file, UVs normalised by entry header size, '@' entries get blank textures.
- `src/gfx.rs` — `Rect`, `PLAYFIELD` (32,16,384x448), `set_viewport` (per-batch scissor), `sprite()` with UVs, texture freeing; `Mat4`/`Camera::default_2d()` (LH look-at + 30° perspective; test checks (x,−y,0) → pixel (x,y)).
- `src/anmvm.rs` — ANM script VM per 05_anm_vm.md: `Timer` (game-speed fractions), all opcodes −1..89, variables/literal-arg writes into the shared script, interrupts (labels, catch-all, op 89 return), 7 interpolation slots + easing, angular velocity, scale growth, UV scroll. `disasm()`; tests incl. every opcode/size/jump target in all 1151 scripts and running each script 600 frames with interrupts (TH08_DIR).
- `src/anmmgr.rs` — `set_script`/`start_script` (init + frame-0 run), draw modes 0–5 (snapping, rotation, anchors, mode-5 world matrix through `Camera::set_2d`/stage camera), EmitQuad (screen offset, D3D→wgpu +0.5 pixel centre, viewport cull, UV offset, colour/colour2, tint, blend bits incl. "keep previous"). `src/rng.rs` placeholder RNG (SPEC-GAP until the RNG spec).
- Title runs title01.anm scripts 0–9 (side logo + nine menu items) over title00.png.
- Debug: `TH08_ANM=<file>:<a-b,c>[:pf]` runs scripts live in slot 24 (`TH08_ANM_INT=<frame>:<id>,…`, `TH08_ANM_POS=x,y[,dx]`, `TH08_ANM_MODE=n`); `anmview <dat> <x.anm> --dis` prints sprites and disassembled scripts.

- `src/stdfile.rs` — .std parser (header, BGM names, objects/quads incl. ribbons, instances, 20-byte script); test checks all 18 files (384 sprite / 168 ribbon quads). `cargo run --release --bin stdview <th08.dat> <x.std>` dumps one.
- `src/stage.rs` — Stage subsystem per 06_std.md (calc 8, draw 6/7, ANM slot 4): Stage_Load (one VM per quad), STD script VM (all opcodes 0–34, halt/jump/section requests), camera interpolation (easing + Hermite), fov, sway, fog interpolation, object update (tag-1 colour2), VMs A/B/C (VM C → clear colour), Stage_DrawLayer culling, 3D world quads, billboards (software fog, projected depth) and ribbons. Tests: synthetic camera/jump/halt/section/fog, every `.std` (incl. `_s`) run 6000 frames.
- `src/gfx.rs` — vertices are clip-space (x,y,z,w) + per-vertex linear fog; depth buffer (Depth32Float, cleared per frame), ZFUNC LESSEQUAL/ALWAYS (`set_depth_test`), per-quad z-write, viewport clears (`clear_depth`), alpha test (alpha ≥ 4). `clip_in_viewport` maps a D3D viewport's clip space into the 640x480 target (real D3D viewport, not just a scissor).
- `src/anmmgr.rs` — `draw_3d_world` (Draw3DWorld: world matrix, texture-matrix upload quirk, vertex fog, z-write flag), `draw_mode_z` (2D modes with explicit depth).
- Title BGM is started by the title task after title00.png loads (00_overview §3.2), not at app start.
- Debug: `TH08_STAGE=<1|2|3|4a|4b|5|6a|6b|ex>` (also `stageN[_s][.std]`, `idx:<0..17>`) runs that stage's background alone over black and starts BGM slot 0; `TH08_STAGE_FRAME=<n>` fast-forwards n stage frames on the first calc (use with `TH08_SHOT=1`). All stages verified visually at frames 60–9000.

- `src/settings.rs` (was `src/env.rs`, impl31) — debug settings: env vars natively, URL query on the web (`?stage=4a&stage_frame=600`, `?anm=title01.anm:0-9`, `?anm_int=…`, `?slot_test=…`; key = env name without `TH08_`, lowercase). Web `?shot=<n>` freezes logic at frame n (native `TH08_SHOT` exits) so pages can be screenshotted.
- Web parity checked (2026-10-17): title (frame 120), `?anm=title01.anm:0-9`, stages 1/3/4a/6b (frame 600) are pixel-identical to native `TH08_SHOT` captures on WebGL2 (Playwright MCP Chromium) and WebGPU (headed Chromium, NVIDIA Vulkan). Title BGM starts after the first key press (AudioContext running). No backend changes were needed.
- Ground-tile seams on stages 1/3 investigated: geometry is watertight (shared edges project to bit-identical vertices), sampling stays inside the sprite rect (stage 1 row 339 reads texel row 1.52 of sprite (1,1) 254x254), clamp/point sampling don't change them, a white texture removes them → the sprite's first and last rows simply don't match in the data. Not a renderer bug.

- `src/ecl.rs` — .ecl parser (header, timelines, sub table, 12-byte sub / 8-byte timeline instructions, literal write-back into the loaded file) + argument signatures for ops 0–184. Test: all 24 files, every instruction size matches its signature, jumps land on instructions, sub ids valid. `cargo run --release --bin eclview <th08.dat> <x.ecl> [sub]` disassembles.
- `src/eclvm.rs` — Ecl_Run per 07_ecl.md: main context + 4 threads with own call stacks, exact-time clock matching, wait, rank masks (incl. human/youkai bits), calls/returns (8 argument registers), interrupts skipping the pending instruction, all variables 10000–10100 with literal-destination write-back, 8 interpolation slots (linear/eased/Hermite, position → velocity), every opcode 0–184, natives 0–31 (4/7/9/11/13/16/21/25 by inferred readings, impl72).
- `src/enemy.rs` — EnemyManager (calc 11, draw 8 for layers 0–1 / 11 for layers ≥ 2, ANM slots 7 `enemy.anm` / 8 `stgNenm.anm`): template defaults, timelines (all ops, stall, sync slots, boss waits, spawn blocking), spawn (+ immediate first run), movement modes 0–3 with mirror/clamp/follow, screen check, life thresholds / timeout re-runs, damage rules, death modes, drops (every-third table), child chains, boss slots, spell-card state + bonus decay, stage section requests (op 147 → `Stage::section_request`).
- `src/bullet.rs` — BulletManager per 08 (calc 14, draw 13, `etama.anm` slot 6): 21 templates (5 VMs each, hitbox/size class from sprite height), 1536 slots + sentinel, visit order 0,1535..1, size-class LIFO draw lists, PlaceBullet (cursor, colour/effect recolouring, spawn effects, cancel guard), states 1–5, every extra command of op 111 incl. chaining, flag gating, sub-patterns and the motion handlers, off-screen counter rules, graze/hit/bomb-area collision, lasers (all states, rotated-rectangle collision with the startup/fade width/2 quirk, emitter), CancelAll/CancelBonus/CancelRadius, ghost toggle (natives 12/14). ECL calls SpawnPattern/laser spawn/items directly (no queues).
- `src/item.rs` — ItemManager per 08 §8: 2096 slots, spawn-order list, spawn modes 0/1/2/3/5 (type 10 → time orb, power → converted at full power, one-probe time orbs), per-state update (fly-to-target, gravity, collection line, magnet), collection effects (power levels, point value formula, extends table, bombs, 1up, stars, time orbs), draw with off-top arrows. Wired to enemy drops, ECL item ops and bullet/laser cancels.
- `src/player.rs` — (stub replaced, see the player entry below).
- Phase-3 answers applied: global tint = D3DCOLOR + enable flag (05 §12.1); Rng_Range unsigned, n = 0 draws nothing (§12.3); blend state carries over frames, texture/texture-matrix caches reset in Stage_DrawLow, Draw3DWorld keeps the bound texture on a cache hit (§12.5, §12.8); slow-down restore flag makes Timer decrements step twice (§12.6, native 29); mode-5 anchor uses |half size| (§12.7); stage overlay VM = text.anm script 33 driven by the player mode byte (human blue tint) and ribbon skip rules (06 §13).
- Tests: ex-command cases (speed-up, accel, angular, redirects, bounce, wrap, chaining/gating, sub-pattern), off-screen rules, cancel → items, laser lifecycle, item spawn/collect, point-value/extend formulas; every stage's ECL 6000 frames on Easy/Lunatic with real bullets/items, laser stages 20000 frames.
- Debug: `TH08_STAGE=<n>` (web `?stage=`) now also runs the stage's ECL with enemies, bullets, items and the stub player (arrows/shift); `TH08_RANK=<easy|normal|hard|lunatic|extra>` (default normal, extra on the Extra stage); `TH08_ENEMY_DMG=<n>` deals n damage per frame to every on-screen shootable enemy (to see boss phases/deaths). `TH08_STAGE_FRAME` fast-forwards enemies/bullets too. Verified by screenshots of all 9 stages at frames 1500/3600/7000 (fairies, midbosses and bosses appear and fire; Stage 1: midboss ≈ frame 3000, boss ≈ 5600).

- `src/player.rs` — Player per 09 (calc 9, draw 9 bullets/options/sprite, draw 10 bomb + focused hitbox; `playerNN.anm` slot 5): game input word (09 §3, `src/input.rs` now uses that bit layout), direction codes, focus/mode byte (> 6 frames; solos by parity), sht speeds × speed multipliers, clamp 8..376 / 16..432, hitbox/graze/item boxes from the human sht, sprite scripts 0–9 (start/stop sets), states 0–3, Player_Die (deathbomb window n per bombs/time/spell/shot type, bullet freeze during spell cards), dying (−15 time per frame, real death: time loss, power −16 / 0, drops in mode 2, rank −1600, 30-frame shrink), respawn (60-frame clear rect, grow, 240 invulnerability, counter := sht +8), invulnerability blink, gauge limits per shot type, gauge drift/shooting timers, stats setters with checksum RNG draws (SetLives/AddBombs/SetPower/AddTime with clamp + max-point gain, ScrambleDecoys on hits).
- `src/sht.rs` — full .sht parser (header, levels, 0x38-byte entries) for all 8 files; `files(shot)` / `team(shot)`.
- `src/pshot.rs` — player bullets (128) and options: shot timer 0..19 cycle, firing walk (slots × entries), init fns 0–8 (aim at nearest enemy, laser slots, option angle, RNG spread), update fns 1 (homing) / 3 (missile, RNG) / 4–5 (Alice laser), hit fns 1 (explosion, RNG) / 2, hit state (+11 script, vel/8), Player_DamageAgainst (bomb ÷5, shot cap 50, time-orb accumulator, damage areas with period/max, youkai-max ×1.06). Options: Yukari, Alice, Remilia ×4, Yuyuko ×2, Youmu's half; team swap on focus, leave via interrupt 5.
- `src/bomb.rs` — 192 + 192 damage/cancel areas (circle/rotated rect, growth, life, period, max), Player_UpdateBomb (start conditions, last spell from the deathbomb window = other member's LS using 2/all bombs, counter +6, rank −200, gauge ±26000/duration, speed multipliers) and all 16 character bombs + the dissolve spell with the 09 §7 durations/invulnerability/areas (choreography simplified, SPEC-GAP). Areas are drawn as additive shapes.
- `src/enemy.rs` — player collision per 07 §10.1 step 6: contact kill (box × 0.7), bomb-area touch (−10 life), shot/bomb damage through `Player::damage_against` (spell-card bomb rules), nearest-enemy target for homing/aimed shots; stage start sets max point value / time threshold and draws EnemyManager_Load's 2 RNG values.
- `src/rng.rs` — g_Rng per 11 §1 (u16 state, rotate-left-2, Next32 = hi then lo, Range/Range16/Float/Signed, call counter); boot seed = low 16 bits of the clock, `TH08_SEED=<n>` fixes it.
- Debug: `TH08_PLAYER=<0–11 | reimu | yukari | … | ry | ma | sr | yy>` picks the shot type in `TH08_STAGE` mode (playable with the keyboard: arrows, Z, X, Shift); `TH08_POWER=<0–128>`; `TH08_INPUT=<file or inline>` replays per-frame input words (`frames:mask`, comma/newline separated, e.g. `100:0x01,1:0x03,1000:0x05`). Tests: movement speeds/bounds/multipliers, mode delay, collisions, deathbomb window + death + respawn timings, deathbomb last spell, power/lives/time rules, gauge limits, all .sht files, RNG sequence, input scripts, every shot type through a stage for 4000 frames (`TH08_TEST_FRAMES`). Screenshots checked: all 7 team shot sets, focused Yukari option, all 8 solo bombs, death → shrink + drops → respawn → invulnerability.

- `src/msg.rs` — msg*.dat per 10: file table (stage × team), face files per stage (6B/Extra swap on entry 0), parser (XOR-0x77 Shift-JIS strings), Msg_Start (state reset, stage-5 entry 10 → Final A / Final B / choice menu from continues + clear masks, 6B late-entry rule, playfield side effects), Msg_Run (release counter, Ctrl skip, item auto-collect, all 23 opcodes incl. wait min-wait 6/8/30, speaker highlight interrupts 3/4/6, choice menu op 21 + jump op 22). Timeline op 6 starts it, op 7 stalls while blocking; dialogue blocks player shot start and bombing.
- `src/text.rs` — Text_RenderToVm rasteriser: 2× work surface, 4-offset black outline, speaker colour, 2:1 box downsample into the `text.anm` render target (`Gfx::write_texture`). Font: `TH08_FONT`, else MS Gothic / Noto Sans CJK / Droid paths natively; canvas 2D `fillText` on the web.
- `src/gamemgr.rs` — `Game` (persistent GM state in `Ctx`: scene, difficulty, shot, stage, flags, rank, clear masks) and GameManager task (calc 2): new game / next stage start (RNG jitter + decoys + checksums, stats defaults, time thresholds, rank table), stats carried between stages through the player, stage-end state machine (clear masks, Stage_Advance 4A/4B and 6A/6B, ending / result / night-over), loading with "Press Shot Button" wait after a clear, pause (ESC), game over → continue menu, score roll, rank sub-counter → ECL rank var, continue side effects.
- `src/ascii.rs` — ASCII manager (calc 1, draw 14/20): text queue with `ascii.anm` glyphs, pause menu (12 §3: states, confirm screens, ESC/Q/R shortcuts, quit / retry scenes) and continue menu (11 §4.2 / 12 §4: ending when the night is over, Yes → +1 clock, continue resets; No → result).
- `src/hud.rs` — HUD (calc 15, draw 17): front.anm side panel (tiles, labels, hiscore/score + continue digit, life/bomb icons, power gauge, graze, point/extend, time/threshold), stage title + song card (stgNtxt) and night clock (times.anm, ECL 179/181), dialogue drawing (portraits by z, growing text box, lines, boss name card), boss UI (life bar ±0.01/0.02, markers (ECL 158), remaining-bars gauge (ECL 148), timer colours, spell name/bonus), popups (bonus, messages, spell card bonus), stage-clear panel with bonus computation and animated clock that fires the clear trigger, op 7 BGM change, op 12 fade (op 14 flash = ScreenEffect type 4 since impl30).
- `src/supervisor.rs` — scene switching per 11 §7 (0 boot, 1 menu, 2 game, 3 next stage, 6 result, 9 ending, 10 retry, 11 reload); the main menu (`src/menu.rs`) starts games; ending is a text stub (result: impl23). `TH08_STAGE` now starts a full game at that stage.
- `src/audio.rs` — BGM pause/resume and linear fade-out.
- Spell end (ECL 123): capture → score bonus, spells +1, time bonus formula (11 §8), popups; enemies freed by the dialogue's destroy step when they have no death sub left (4A/4B midboss).
- Debug: `TH08_INVULN=1` (no deaths), `TH08_TURBO=<n>` (n logic frames per drawn frame, no pacing). Verified: dialogue on the stage 1 boss, HUD during play, pause menu, continue menu (Yes → continue), stage-clear panel + "Press Shot Button", 4A/4B by team, Final A/B choice (solo) → 6B, full scripted runs 1 → 2 → 3 → 4A/4B → 5 → 6A → ending stub → result stub → menu → new game, Extra → result. Tests: routing, clock advance, rank, score roll, every msg file, wait/skip/release, final-route resolution.

- `src/menu.rs` — main menu per 12 §1–2 (calc 4, draw 3, `title01.anm` slot 20, 142 VMs): top menu (locked items skipped, descriptions, Quit fade + exit), difficulty, character/team select (Extra unlock, 12 choices), practice stage select (ASCII list), option screen bound to `src/config.rs` (lives/bombs/graphic/BGM/SE volumes with the 1 − (1 − v/100)³ curve, mode, slow, reset); key config, replay and spell practice are stubs that return. Key repeat (30 frames, every 8) in `src/input.rs`. Returning from a practice game reopens the practice screens. `src/supervisor.rs`: boot logo (th08logo.jpg + nowloading, 60 frames) → menu; Result / Ending / Music Room scenes. `TH08_UNLOCK=1` unlocks everything.
- `src/sound.rs` + `src/audio.rs` — SE per 13 §1 + 10 §10: 46-entry table (wave, 1/100 dB), per-frame queue (12 ids, 128 pans each, average pan), Sound_PlayAt pan from x, wav parser (PCM 8/16, mono/stereo), one voice per id mixed in the cpal callback (web too) with dB → gain and pan attenuation, SE / BGM volume from the config. Hooked at shots, graze, death, extend, bombs, ECL 124/181/children/spell start, enemy damage/death/mode change, spell end/capture, bullet spawn/ex sounds, items/power-up, boss timer warnings, HUD events, pause/continue/menu cursor. `TH08_SE_LOG=1` logs the flushed queue per frame.
- `src/effect.rs` — effect manager per 13 §2 (calc 13, draw 12): 653 slots (ring cursor 0–511, high 512–639, fixed 640–652), the 66-type table with per-type init (RNG draws as specified) / update, draw lists A–E (list D after bullets, B billboards in the stage high draw), ring renderer (plain / ellipse / wavy). Spawns: shot hits (3, 5), enemy death bursts and drops, player hit/death (6 ×16, rings 59 / 12), graze, focus switch / youkai aura / gauge-max aura, bomb invulnerability ring, power conversion, ECL 128/139/140/174, natives 3/6/20 and 5, spell-card ring 39/52 (+ capture recolour). ScreenEffect types 0–7 (fades, flash, shake with 2 RNG draws/frame) and ECL natives 1/10/15/17. Spell-card background (13 §5): effNN.anm slot 9, two VMs, stage layers hidden after 60 frames, removed by Spell_End (also on boss death). Score popups (13 §4) in `src/ascii.rs`: 720 + 3 slots, rise 0.5/frame, 60 frames, fade sprites, alpha by distance to the player; callers: items, cancel bonus, kill_all.
- Debug: `TH08_SCREEN_FX=<frame>:<type>,<p0>,<p1>,<p2>,<p3>` queues a screen effect at that game frame. Verified by screenshots: boot logo, top menu, difficulty, character select (team + solo), options, practice list, game from the menu, enemy death burst, player death ring + sparks, spell background + ring (stage 1 boss), flash, shake (ANM sprites + text offset), popups. Open questions for the dirty room: `specs/IMPL9_QUESTIONS.md` (hit-spark rate, spell-ring texture axis, …).

- impl10 (answers_impl9 + answers_phase5, 2026-10-17):
  - Hit spark type 3 only from damage areas (bomb / last spell): shared u8 player counter, every 4th hit, no spark for Reimu/Marisa bomb areas (`Area::no_spark`); bullets spawn type 5 only (hit state, explosion filter timer % 6, filter 2 counter & 7).
  - Ring renderer: v along the ring (v1 → v0 over the circle), u across the band, WRAP; plain/ellipse/wavy positions; spell ring 39/52 = plain ring (only width/radius copied), radius 8 → 256 ease 6; death ring 59: 64 segments, radius 128 → 8 over the deathbomb window (ease 4), runs while bullets are frozen. Not shaken.
  - Effects: 3D particles 19/51/63 (10 / 8 RNG draws, camera + facing/2 base, view-cone cull, 19 freed at z ≥ 0, 51/63 boss pull + player push screen helper, 51 colour2 × stage overlay colour), std op 5 shifts kind-51 effects; op 128 orbit effect 13 (quaternion rotation, growth 0.3/frame to d, spin, fade); list B only at effect level 2, layer 4 at raw screen coordinates; youkai aura / gauge aura / invulnerability ring follow the player per §9.
  - Natives 3/6/20 stage mask callback (black square frame + mirrored spell background, `Stage::draw_spell_mask`); natives 12/14 (blend, native 14 three-step fade cycle, spell-bg interrupts).
  - Stage sparkles (tag 0x1X billboard points → type 0x3E at 12 points every 3rd frame after 700); shake offset zeroed after the prio-14 popup draw.
  - Popups: colours (max 0xFFFFFF00, PowerUp 0xFFFFC0A0, time orb 0xDFFFEF80 / 0xDFFFFFFF), top-left glyph anchor and fade sprites 21/31 for PowerUp, cancel star / time orb in the 3 big slots; large popups (+n / +n(m)) from chain kills and the boss counter (`ascii::popup_large*`).
  - Menu: description = text.anm script 9 at (320, 448); option value VMs (lit VM = first + value, lives max by play count, digit sprites base + 2·digit); unlock checks per 15 §2.4 on the session masks; the stub ending sets the Final A/B bits; 15 §1.1 config defaults; title fade-from-white (type 0, 70 frames).
  - Phase 5: enemy draw (sub 0 / main / sub 1, Draw2DRot, rotation rules, red tint while frozen), bullet draw (z 0.05, RGB white, tagged rotation, tint), item z 0.15, contact (graze hitbox/0.7 every 6 frames for bit-7 enemies, hit hitbox/1.5, −10 life), shot box second damage call, bomb damage in cards (bit 7 / 2.5), spell flags (capture, timed out → cancel mode 4, bombed → bonus 0) and Spell_End (CancelBonus + KillAll + Hud_ShowBonus, 70-frame invulnerability), drops in mode 1 when killed while bombing, homing/aimed targets, random direction 0x422020/0x4224A0, op 169, interrupt depth/main-context rules, HUD boss byte cleared by any boss slot, rank +100 timer, over-80% counters + clear-panel lines, shot cycle blocked during dialogue, continue default "No", hit tests return 1 in every player state (only state 0 dies), input bits Q/S/Home/D/R, boss name plate copied from ename.png (`Gfx::copy_texture`), hitbox marker = effect 22, shot red tint (FF,40,40).
  - The "destroy frees doomed midboss" workaround was removed (the 4A midboss deletes itself); full scripted runs 1 → 6A → ending → result → menu, 4A → 6A, 6B, Extra verified. SPEC-GAPs 79 → 35. Questions: `docs/briefs/IMPL10_QUESTIONS.md`.

- impl11 (14 §1–6, 2026-10-17):
  - `src/bomb.rs` rewritten: 128 persistent bomb objects (fields kept between bombs), Bomb_Start (state 3 + invulnerability ring, SE 0xE, attract, origin), "at T = n" = timer advanced, all 16 bombs + dissolve spell per 14 §2 (Reimu orbs homing/impact, LS blasts on objects 16/17 with RNG points, Alice doll = option 0 with rings 0x28/0x2A–0x2D, Master/Final Spark beams in object 0's 5 VMs + rings 0x35, Sakuya knives (3 RNG each, hover/re-aim at age 70), Remilia anchors + rings, Youmu dash/slashes/return + white/red playfield rectangle, Yukari boundaries 0x24/0x25, Yuyuko butterflies (LS free-slot quirks)), per-area no-spark flag, screen shakes/flashes, background tint 0x40BC60 → `AnmManager::stage_tint` applied in Stage_DrawLow, cleared after Stage_DrawHigh. Bomb start also frees the death ring (09 §7).
  - `src/effect.rs`: bomb-owned ring updates (36/37 boundaries → 4 rotated side rectangles queued into the player's areas via `bomb::queue_area`, 42–45 Alice, 46 Sakuya LS, 48 Youmu slash); `src/anmvm.rs` helpers with explicit start/end (pos/scale/alpha/colour).
  - Options (14 §3): state 1 init → calc fns, team focus rewrites all 4 slots, unfocus sends them away + Youmu's half with trail reset; Yukari smoothing/attack/mirroring, Alice, Remilia team (absolute anchors) / solo (follow while unfocused, colours), Yuyuko, Youmu half/solo (trail[15] anchor, facing, colours), sparkle effect 0x2F; draw z 0.49 Draw2DRot. Shot init 8 uses option 2's facing.
  - Alice laser (14 §4): 3 laser slots with entry check, update 5 (trail of 32, one-frame 16 × 448 damage rects, follows option 0, ends on release/dialogue/bomb/death/level change), trail draw. Player draw order per 14 §5 (prio 9 flying bullets z 0.4 + trail, bomb, sprite z 0.1 Draw2DSnap, options; prio 10 hit-state bullets z 0.2, same rotation rule — fixes VISUAL_BUGS_1 B1).
  - VISUAL_BUGS_1 (answers_bugs1): B1 hit transition keeps the VM rotation across the hit-script restart (hit script re-sets the tag, drawn rotated by the bullet angle); B2 death ring 59 (slot 11) spawns only with ≥ 1 bomb, is freed when the deathbomb window ends (before the real-death effects) and on bomb start; real-death burst = type 12 in slot 3 (`FIX_DEATH_BURST`), colour 0xFF4040FF, 40-frame life. Checked against specs/data/ref/b2_*.
  - Debug: `TH08_BOMB=<0–4>` forces the bomb index (4 = dissolve spell). Questions: `docs/briefs/IMPL11_QUESTIONS.md`.

- impl12 (answers_impl10, answers_impl11, 12 §5.8, 2026-10-18):
  - Large popups: no motion, x0 = pos.x − count·3.5, advance 7·scale, top-left, player-distance alpha; LargeStatic adds the playfield origin + count·3.5; HUD "+p3(p4)" every boss frame (also "+0").
  - `Enemy_KillChildren` rewrite (`src/enemy.rs`): chain kill (colour by n, follow-offset fix, unlink, cancel circles r 32, 2·n-based time-orb showers with 2 RNG draws, ÷3 compounding while bombing, drops when no boss / in a card, SE by parity), boss part (×2 popup, 2·count orbs, cancel circle, gauge lock 0), single kill of a chained child (gauge −1/12, timers, "+1", time orb, unlink, parent count −1); also from `Enemy_Free` without reward. Time-orb gauge lock = `ItemManager::collect_timer`.
  - Native 12 pauses the caller's chain (flags B bit 7: ECL skipped, boss timer held), also flag A bit 30 while bombing / player state ≠ 0.
  - Name plates: `hud::stage_plate` with the spell-practice exceptions (`Game::card`, `is_practice_card(_in)`), `hud::set_boss_plate`; Msg_Start entry 0 hooks (stage 5 Reisen, 6A/6B/Ex spell-bg base 2, 6B Kaguya, Ex Mokou plates) via msg events.
  - Last spell / special mode: GameFlags bits 7/8/13/14 mirrored in `Player::gflags` (op 176, Spell_Start bit 5 → "Last Spell Failed", Spell_End capture → bit 8); special mode: bomb press = SE 0x29, hit → forced dissolve spell (index 4, 40/30-frame practice variant), start clears bosses' flag A bit 6, end clears 7–8, kills boss chains + boss life 0 + bit 30, white flash.
  - Explosion hit filter per answers_impl10 §8 (script 0xC–0x14 → 48–128 hitbox, RNG always drawn, type-5 spark after both branches).
  - Bomb tint lifetime: `AnmManager::{request_tint (averaging), tint_off, bomb_drew}`; DrawHigh keeps the tint while a bomb drew last frame (stage, list B, spell bg, prio-8 enemies, flying shots tinted); prio-11 enemies / bullets / Gui set red only while frozen, then off; bomb end and dying turn it off.
  - Bombs/options: Reimu orb areas follow during the spin-out, Reimu LS start effect 12, Sakuya LS SE 6 after the knife loop, Yukari LS slot 4 script 88, Alice rings freed by script 83, Remilia ring width interpolation, Yuyuko sparkle 0x80602050, persistent Yukari aimed enemy (`Player::aimed` = enemy index, replaced by a higher parentless enemy in the 64-px column).
  - `src/gui.rs` (12 §5.8, calc 12 / draw 15): 14 VMs, player cut-in (face slot 16/17) + cdbg strips + card name (text.anm 4, left) + plate; boss cut-in (slot 18, sprite = face) + name (text.anm 5/6–8, right-aligned) + plate + "Bonus … history" label with bonus/captures/attempts digits; strips only while a player cut-in is visible; natives 22/23. Old HUD spell-name code removed.
  - Verified: spell cut-in / name / bonus line vs `spell54_cutin_f134.png` and `_under_fire_f209.png` (stage 1 card), player plate + strips (Yukari bomb), Remilia bomb tint on fairies, "+0(3)" boss counter, full scripted runs 1 → 6A → ending → result → menu → 1 → 6B, Extra; tests; wasm check. SPEC-GAPs 34 → 24. Questions: `docs/briefs/IMPL12_QUESTIONS.md`.

- impl13 (VISUAL_BUGS_2 / answers_bugs2, 2026-10-18):
  - C1 menu seam: every frame renders into a fixed 640x480 target, then a linear blit letterboxes it into the window (05 §12.4: snapped 2D quads sample texel centres only at 640x480). The window is no longer rendered at its own resolution, so this applies to all draws. `TH08_SHOT_SIZE=WxH` sends the shot through the same blit. The Practice Start band matches `c1_menu_practice_sel.png` within ±2.
  - C2 character select (12 §2.4): difficulty plate (VM 131 + config+0x21 gets interrupt 9 after the range 8 on screens 5/9/13), Final-route notice (sprites 145–148 from the cursor team's CLRD masks, (400, 170), top-left, screen 5, frame > 8). Plate and notice are pixel-identical to the `c2_*` refs. Debug `TH08_CLRD=<shot>,<any>[,<nc>]` sets that shot's clear masks on every difficulty.
  - C3 gauge readout (12 §5.9, `src/ascii.rs`): 5 VMs (ascii.anm scripts 5/6/7/8/4), setup (AsciiManager load, every stage start, menu register; 人/妖 x from the gauge limits; stored interrupt re-sent), interrupts 1 (fresh Player_Load) / 99 (player removed, requested scene not 3/11/12) / 2–3 (corner fade from Player calc), draw at prio 14 after the popups (marker, zone tint, "%d.%02d%%" with 0.8× decimals, max point value). Matches the layout and colours of the `c3_demo0_*` refs. Type-4 death/drop sparks and time orbs already matched 13 §3 / script 68 (checked visually).
  - Verified: screenshots above, full scripted run 1 → 6A → ending → result → menu → new game, `cargo test --release` with TH08_DIR (61), wasm check. Questions: `docs/briefs/IMPL13_QUESTIONS.md`.

- impl14 (15 §2–3, answers_phase7 §3, answers_phase8, 2026-10-18):
  - `src/scoredat.rs` — score.dat: two-layer container (archive block cipher 0x59/0x79/0x100/0xC00 + rolling XOR, checksum, LZSS; `archive::{block_encrypt, lzss_compress}` added), all chunks (TH8K, HSCR insert/top-10, CLRD ×13 incl. union entry 12, CATK with checksum/attempts/captures/max bonus/comments, PSCR, LSNM, FLSP, PLST counters/clocks, VRSM kept as loaded). The real score.dat re-encodes to a byte-identical body. Save = 2 RNG draws: game start flush (GameMgr_LoadScoreData), pause quit, result stub removal, exit. Storage never in the game dir: `TH08_SAVE_DIR`, else `$XDG_CONFIG_HOME/th08` / `~/.config/th08`, else next to the binary; web: localStorage (hex). First boot imports the original's score.dat read-only. Clear masks, unlocks (GM flags 15–17), lives-option limit (PLST plays) and the Gui bonus-line history (CATK) now come from it; `TH08_UNLOCK` still overrides.
  - `src/replay.rs` — .rpy load/save (header crypt `b += key, key += 7`, checksum, LZSS, info block, per-stage records + fps streams); recording node prio 17 (g_Input := raw at frame end → gameplay reads `Input::game_view()` with the original 1-frame latency), playback prio 6 (dummy word, key-repeat emulation, fps byte q+1 quirk), stage load from the record (stats, rank, seed), next recorded stage / scene 7, fast-forward node prio 18 (dialogue 3×, speed 2 fast), slowdown reproduction in GM calc. Attract demo from the title after 1500 idle frames (demorpy0→3, GM+0x3DBAA = shot % 2, 80 loading frames, demo limits/fade, any key → menu). Replay screen (ASCII list of `replay/th8_NN.rpy` + `th8_ud*.rpy`, stage + speed choice). `TH08_REPLAY=<file|demoN>` (+ `TH08_REPLAY_SPEED`) starts a replay; `TH08_REPLAY_SAVE=<1–15>` saves the recording when the game ends (no save dialogue yet).
  - `src/demosync.rs` + `cargo run --release --bin demosync <0-3> [from-to] [rng-frame]` — demo sync check against `specs/data/demo*_trace.txt` (first divergence per field; per-frame rows; RNG call sites of one frame via `rng::TRACE`). Test `demo_sync` asserts a minimum sync per demo; `record_playback` records 3000 scripted Lunatic frames, round-trips the .rpy and checks the playback matches every frame. `Gfx::headless()` for tests/tools.
  - Fixes found by the traces: rank changes applied immediately (global rank state, ECL var 10041), max point value no longer reset per stage by the enemy manager, focus switch: marker effect 22 on the switch frame + flash only after > 3 frames, enemy death gauge ±200 by mode, graze score 4000 at gauge ≥ youkai, point-item checksum, leaving options skip their calc fn, child aura 0x20 on ops 90–92. Sync: demo0 260 / demo1 559 / demo2 449 / demo3 794 frames (was 0); RNG state before frame 1 matches in all four. Questions: `docs/briefs/IMPL14_QUESTIONS.md`.

- impl15 (draw log, specs/data/drawlog, 2026-10-18):
  - `src/drawlog.rs` — port draw log per specs/data/drawlog/FORMAT.md: `Gfx` records every quad (texture label `<anm>#<entry>` / image name + logical size, blend, depth test/write, viewport, fog parameters, vertices back to D3D screen space), clears and copies (`Gfx::image` → `K <img> screen …`, `copy_texture` → K); `Ctx::draw` writes one `F` record per presented frame (scene, scene frame, game frame, demo, demo frame, input). `TH08_DRAWLOG=<path>[,from-to | ,dfrom-to]` (`.xz` compressed in-process via liblzma, `src/xz.rs`); works headless. Reader (plain / xz, demo chunks concatenated), per-frame diff (pass 1 exact by sprite + position, pass 2 nearest of the same texture within 64 px → changed: pos / uv / colour / z / state; 2D z/zwrite reported unscored; draw-order inversions via LIS; P strips expanded to quads; fps corner dropped; `--playfield` filter), session report (scene key with visits + title-intro ±1 slack, demo key = demo frame streaming merge, sprite-set summaries).
  - `cargo run --release --bin drawrun -- <out[.xz]> menu <input.txt> [clr] | demo <n> [last]` (headless port run; menu script frame 1 = first menu frame, menu_clr CLRD pokes at 100/280). `cargo run --release --bin drawdiff -- <reference> <port-log> [from-to] [--playfield|--full] [--key scene|demo|present] [--worst N] [--out DIR]` → summary, worst frames, `DIR/frames.txt`, `DIR/index.html` (reference and port logs rendered by the port's renderer with back-buffer persistence, red pixel-diff, reference PNGs of menu_ref shots and demo `dl_*` frames). `<dir>/demo0` expands to all chunks.
  - Tests `drawlog::tests` (format round trip, menu_base, menu_clr, demos 0–3 first chunk; skipped without TH08_DIR/GPU/chunks), thresholds in `MENU_BASE` / `MENU_CLR` / `DEMO`.
  - Fixes from the diff: menu loader phase (boot 13 frames in scene 0, then logo + nowloading in scene 1 until scene frame 34 / 33 on re-entry, title fade started at its end; loading screen also on later menu entries), nowloading VMs drawn Draw2DRot relative to (500, 440) incl. pos_offset/z; ScreenEffect calc removal also removes its draw node (no alpha-0 rect forever), type 0 alpha = trunc(255 − 255·t/dur) in float, t = 0 alpha before the first calc; `Gfx::rect` at D3D coordinates (+0.5); 2D draws carry the VM's z; D3D-coordinate culling; menu: cursor keys handled before highlights/interrupts (press shows in its own frame), difficulty cursor interrupts 24/25 sent on entry, character select interrupts on its first frame, no interrupt 6 on character cancel, delayed screen switches start the frame after the counter ends (press + 21), locked items colour 0xFF404040 incl. alpha, item descriptions rendered into text.anm sprites 21 + i with the per-item VM rules of answers_impl9 §15 (fade-in from frame 9 on re-entry, interrupt 2 when leaving); popups + gauge readout clipped to the playfield; power bar at D3D coordinates with z 0.1; input field logs the held word outside gameplay.
  - Match: menu_base 1257/1902 frames identical (frames 76–1122 all identical; rest = boot race, practice-stage list, replay screen, result/music stubs), 49.7 % quads; menu_clr 859/982, 96.6 %; demos first chunk (playfield) 73.2 / 21.1 / 52.1 / 52.0 %. Questions: `docs/briefs/IMPL15_QUESTIONS.md`.

- impl16 (answers_impl14, demo sync push, 2026-10-18):
  - `Enemy_ReactPlayerMode` (07 §10.1 step 3): human→youkai effect 0x1F / aura interrupt 2 / sfx 0x28 / layer 0, youkai→human 0x1E / 1 / 0x27 / layer 2, effect 38 for op-83 enemies in human state every 2nd timer tick. Child setup: layer 2 human / 0 youkai, aura interrupt 1 human / 2 youkai, only if no aura yet.
  - Effects: the script's t = 0 step (and its RNG) runs before the type init's RNG (13 §2.6; `reset` → `start_script` → `type_init` → `start_post`).
  - Enemy death: gauge ±200 from the focus byte +3, death sparks → time orb (type 7) at human/youkai max; drop sparks only for item ≥ 0 (×3) or a table drop (×6); `Enemy_KillChildren` orb shower for any parent; child counter not reset.
  - Shot damage uncapped (50 cap on the time-orb accumulator only), one accumulator for all non-boss enemies; every-third-drop counter starts at 2 (SPEC-GAPs, see questions).
  - Leaving options freed at t > 16 (VM still runs that frame), Alice follows the player while leaving; demo music fade 3 s (÷ game speed); PLST: practice counts plays too, order plays → retries → practice, slow byte ignored for Extra/practice; play time marked at every StageStart / pause & continue exit, added at pause & continue entry and at GM removal unless replay; replay stage load keeps the score when no lower record exists; VRSM size/sum constants (840704, 0xA26861B9) in fresh score.dat and replays.
  - `demosync` debug: `TH08_BDUMP=1` (enemies + bullets per frame of the range), `TH08_IDUMP=1` (player + items).
  - Sync: demo0 260→1128, demo1 559→559, demo2 449→723, demo3 794→825 (`MIN_SYNC`); draw log (first chunk, playfield) 73.2/21.1/52.1/52.0 → 83.7/28.6/77.2/66.0 % (`DEMO`). Questions: `docs/briefs/IMPL16_QUESTIONS.md`.

- impl17 (answers_impl15 + spec fixes, main-menu lists, 2026-10-18):
  - Effect 51/63: ring slot = cursor then advance, init offset from −facing/2 with the spec's expression order (matches `specs/data/q15/eff51_decoded.txt` for demos 1 and 3); `demosync` debug `TH08_EDUMP=<type>`.
  - Stage start: loading darken overlay (ScreenEffect type 5 flagged `loading`, released when loading ends, fade 8 frames), deferred gauge readout setup by demo index, "Press Shot Button" after 5 wait frames, nowloading VMs at (500, 440)/(280, 430) with the captured screen (`Gfx::capture_screen`), boot presents (clear only), HUD panel tiles with z-write and per-row z, stage title/clock in the playfield viewport, music-room capture on exit.
  - Menu lists (`src/menulist.rs`): practice stage list (screen 11), spell-practice character screen 16 (rows, totals, four pies via `Gfx::fan`, percent readouts), card list + info panel (screen 17), replay list + stage detail; ASCII queue with highlight glyph set (+0x8A), strips with z-write. `drawlog::menu_start_state` gives tests/drawrun a private save dir (`scoredat::set_save_dir`) with menu_mopup1.score.dat + demo replays as th8_01..04.
  - Match (before → after): menu_base 1257 identical / 49.7 % → 1754 / 70.7 %; menu_clr 859 / 96.6 % → 903 / 97.4 %; menu_mopup1 (new test) 318 / 6.5 % → 1593 / 84.7 % (rest = result-viewer stub); demos first chunk (playfield) 83.7/28.6/77.2/66.0 → 85.2/29.7/78.7/68.0 %; demo sync unchanged (1128/559/723/825). Thresholds `MENU_BASE` (1750, 70.5), `MENU_CLR` (900, 97.3), `MENU_MOPUP1` (1590, 84.5), `DEMO` [85.0, 29.5, 78.5, 67.8].

- impl18 (session replay, 2026-10-18):
  - `src/session.rs` + `cargo run --release --bin session -- <session dir> [--out DIR] [--no-drawlog] [--from N] [--to N]` (headless): start state from `start/` (private save dir with score.dat, replays; `Config::from_bytes` for th08.cfg), boot seed from `events.txt`, `input.txt` as raw input; loader timing from `events.txt` via `Game::sched` (boot length, menu `Loading_End` before the release poll's calc, stage gauge setup / Loading_Wait / release by poll index; demos/replays keep the fixed 30/81 frames). Writes `DIR/trace.txt` (36 columns, same format) and `DIR/draw.drawlog.xz`; compares with `trace.txt.xz` from the first main-menu row (first differing row with old/new values, leading identical rows overall and per scene visit, per-column counts; skips loading rows, `result`, and gameplay columns outside scene 2) and streams a present-keyed draw diff against `sessions/<name>/draw_*.drawlog.xz` (`DIR/index.html`). Debug: `TH08_SESSION_DUMP=a-b` (player, options, enemies, items), `TH08_SESSION_RNG=<row>` (RNG call sites).
  - `src/drawreport.rs`: drawdiff's renderer/HTML moved to the lib; `DemoMerge` also streams `--key present`.
  - Fixes from the traces: difficulty cursor set on Start/Practice confirm, character cursor on difficulty confirm; bomb gauge not on the start frame; shot-timer gate uses the game frame counter (0x164D2C8 = this frame); item state 3 → 1 when the player's shot timer < 0 (was never true); time orbs spawn inside the bullet loop (RNG order); per-enemy time-orb accumulator starting at the step (07 e+0x2E10); focus switch replays the start-move sprite; enemy draw lists linked per calc and drawn newest-first; hit flash not on two consecutive frames.
  - Leading identical rows: golden 1101 → 2172, selftest1 344 → 1325 (test `session::tests::session_sync`, `MIN_LEADING`). Demo sync 1128/559/723/825 → 1317/559/743/1058. Draw logs: menu_base 1754 / 70.66 %, menu_clr 903 / 97.41 %, mopup1 1593 / 84.71 % (unchanged); demos first chunk 85.2/29.7/78.7/68.0 → 86.3/29.8/74.6/78.0 % (`DEMO` [86.0, 29.5, 74.5, 77.8]; demo 2 drops after its trace divergence at 744). Questions: `docs/briefs/IMPL18_QUESTIONS.md`.

- impl19 (answers_impl16 / answers_impl17 Q2/Q5, 2026-10-19):
  - Every-third drop counter / table index = `Rng_Range16(3)` / `Rng_Range16(8)` at each stage load (u16, check then increment, index wraps 32); op 66 / 69 (dur ≥ 1) negate delta.x when mirrored (67/178 not).
  - Stage loading max(31 | 81, ANM entries loaded + 6) (`AnmManager::entries_loaded`); GameMgr_Calc's full-screen z clear moved into the calc (after the demo-end / slowdown returns, before pause/game-over); stg4abg ribbons: colour 1, per-layer-call stale vertex slots (f ≥ 1 clears alpha only, residue 0x910D20 / 0x676E85 / 0 / 0); clock VM ticks only while stage-title VM 0 alpha ≠ 0; replay fps text "%2d" at (384, 448) 0xFFFFFFD0 / 0xFFFF4040, refreshed every 30 gameplay calcs at phase 3.
  - Full-power clamp is a checksummed write (SPEC-GAP, demo 3).
  - Session `meta.txt` line `noclip: 1` → player `debug_invuln` (bin and `session_sync`).
  - Sync: demos 1317/559/743/1058 → 1719/1807/2260/1500 (`MIN_SYNC`); draw logs first chunk 86.3/29.8/74.6/78.0 → 90.5/71.7/83.3/87.0 % (`DEMO` [90.3, 71.5, 83.1, 86.8]); sessions golden 2172 / selftest1 1325 and menus (1754/70.66, 903/97.41, 1593/84.71) unchanged. SPEC-GAP 45 → 41. Questions: `docs/briefs/IMPL19_QUESTIONS.md`.

- impl20 (boss phases, fullrun_noclip, Player_Die order, 2026-10-19):
  - Boss phase bug (stage bosses after their first phase): a mode-2 death set the death latch for good, so in the next phase the boss's life went negative (−1600) without dying and the phase only ended by timeout. Op 131 (`set_life`) now clears the latch (SPEC-GAP; op 129 / death-sub-jump variants checked and rejected). Also: op 36 target decoded as a float var id (interpolations never ran: stage 1 boss familiars flew straight off-screen, their bullets were freed), boss death in mode 1/2 keeps the boss slot + HUD byte until `op127 −1` (SPEC-GAP), timeline `wait_boss` uses the alive bit only, enemy screen check uses the main VM sprite size (SPEC-GAP, IMPL19 Q2), gauge drift skipped during dialogue and the player reads the dialogue state live.
  - Player_Die (09 §6.1): state check → ScrambleDecoys → `Player_Die`; `debug_invuln` returns at its start; it clears the global freeze byte 0x160F534 (now `enemy::timer_pause()`, read live by bullets); HitTestNoBomb sets the bomb-drop item to 6 first. Noclip hit counts match `noclip_hit` events in the synced range.
  - Test `session::tests::fullrun_smoke` (~11 s release): whole fullrun_noclip, no panic/hang; stage starts (menu row) recorded 1:241 2:11287 3:25844 4B:43232 5:68101 6B:89433, port 1:240 2:11208 3:25810 4B:43131 5:72934 6B:107496; boss/spell structure identical in every stage except the Last Spells after the stage 1/3/4B/5 bosses (var 10098 reads 0, IMPL20 Q1); the input ends while the port is still in 6B (original reached the ending at 121239).
  - Leading rows: fullrun_noclip 3861 → 6263 (`MIN_LEADING`), golden 2172 / selftest1 1325 unchanged; demo sync 1719/1807/2260/1500 → 1940/3869/2260/1500 (`MIN_SYNC`); draw logs and menus unchanged (90.49/71.65/83.30/86.97 %, 1754/70.66, 903/97.41, 1593/84.71). SPEC-GAP 41 → 44. Questions: `docs/briefs/IMPL20_QUESTIONS.md`.

- impl21 (answers_impl18 / answers_impl19, 2026-10-19):
  - Aimed enemy: a mode-0 death no longer clears the pointer (only `Enemy_Free` and the next frame's inactive-slot test do). Yukari option: a sub-0 start move replaces the attack test (SPEC-GAP, golden scan f1463). `Player_DamageAgainst` writes hitByBomb = 0 at entry. The bomb gauge also adds on the end frame. Bomb item: `AddBombs` + rank +5. Full-power item: `SetPower` (power-clamp SPEC-GAP closed).
  - Chains: var 10096 = chain entries after this enemy (SPEC-GAP, demo 3 f1501). A chained child freed without reward unlinks itself (answers_impl12 §1). Enemies whose main VM has no sprite get bit 4.
  - Menus: difficulty confirm sets cursor 0 (the character screen's entry sets the shot). The loader leaves description VM 0 running with no interrupt 1, and the cursor's VM takes over from frame 9. Surface 0 is drawn at draw prio 1, before the nowloading quads. paused 1 → 2 on the ASCII calc after the first draw.
  - Hit-flash / draw-list / start-move SPEC-GAPs closed as matching the spec.
  - Leading rows: golden 2172 → 4466, selftest1 1325 → 1422 (the rest is the pause/quit tail, Q3), fullrun_noclip 6263 (unchanged). fullrun stage starts are now 1:240 2:11198 3:25835 4B:43131 5:68252 6B:89394 (recorded 6B 89433), and the port reaches the ending/menu (row 122259; original 121239).
  - Demo sync: 1940/3869/2260/1500 → 4450/3869/2260/1605 (`MIN_SYNC`).
  - Draw logs: demos 90.49 → 90.59 % (others unchanged); menus base 70.66 → 70.81 %, clr 97.41 → 97.79 %, mopup1 84.71 → 84.74 % (identical frames unchanged). Thresholds raised.
  - SPEC-GAP 44 → 40. Questions: `docs/briefs/IMPL21_QUESTIONS.md`.
- impl22 (answers_boss_phases, 07 §4.3/§5.4/§6/§10.1–10.3, answers_impl17 menus, 2026-10-19):
  - Boss phases: a threshold switch runs in the step-4 loop (Ecl_Run → move → screen → thresholds/timeout, same-frame re-run). It sets life/hud to the threshold and bonus seconds, and resets threads, desc, auto-fire, rank ranges and warning bits; the call depth is kept (quirk). Other enemies die and the player is protected for 70 frames. Warning levels in flags B bits 4–5. A timeout only marks the card (timed out, no capture, cancel_all(4)). Death modes 0–3: mode 3 sets life 1 and player state 3 for 90 frames. The death sub runs on the next frame, and the latch is cleared when life > 0. Spell cards keep their own duration; the capture payout comes 130 frames after Spell_End. var 10098 = time + pending + time orbs ≥ threshold.
  - Var 10032/10034 read Next32 as integers (they had gone through f32). Demo 2 f2261 child wait fixed. Op 36 a–d are evaluated once (SPEC-GAP).
  - Menus (answers_impl17 Q1/Q3/Q4/Q7): shot/difficulty names, difficulty lists, unlock hints, Last Word unlocks (flsp), selectable/info panel rules, portrait enable bits (VMs 111–130, 138/139 interrupt 27), screen 17 range interrupt 26, register cursor table. Q6 is a font difference only. Stubs: info notices, key config, the spell-replay card-name VM, and the fading description on non-top screens (SPEC-GAP).
  - Leading rows: golden 4466 → 8055, fullrun_noclip 6263 → 6265 (pb22 homing hit one frame early), selftest1 1422. fullrun stage starts 1:240 2:11252 3:25882 4B:43197 5:73137 6B:107484.
  - Demo sync 4450/3869/2260/1605 → 4450/3869/2937/1605. Demo draw 90.59/71.65/83.30/86.97 → 90.61/71.66/83.33/86.97 %. Menus unchanged (70.81/97.79/84.74 %). Thresholds raised.
  - SPEC-GAP 40 → 31. Questions: `docs/briefs/IMPL22_QUESTIONS.md`.

- impl23 (16 §2.1–2.5, 15 §2.2–2.9, 2026-10-19):
  - `src/result.rs` — Result object (calc 16, draw 18; result.jpg, result00.anm slot 21, resulttext.anm slot 22, 72 VMs, 28 text
    VMs + separator bar). Mode 1 after a game (scene 6): name entry (charset grid with pulse, Up/Down/Left/Right skipping the
    space cell, Shot types / SP / END, Bomb/ESC backspace, ESC finishes, cursor on END when an LSNM chunk was loaded), ranking
    with the new row and cursor underline, end panel (VM 71), stage-practice hiscore (0x11), spell practice (0x16), slow-mode
    skip (CATK backup). Mode 0 viewer (scene 5): top menu, difficulty / character / ranking pages, card groups / character /
    card list (pages, accordion, MaxBonus, captured counts), play-data page (fade in/out, run time refresh), exit via state 2
    (Loading_Begin at frame 1, menu after 20 frames, result.jpg becomes the loading background). Replaces the result stubs.
  - Not done: replay-save dialogue (states 10–14 go straight to the exit; `TH08_REPLAY_SAVE=<slot>` still saves),
    score.txt (D key), hidden unlock code (15 §2.12).
  - score.dat: ranking lists = default table + file entries (`ScoreDat::ranking`), insert keeps lists ordered by difficulty
    then shot; HSCR entry filled from the in-game record (`Stats::rec`: play frames, points, misses, bombs, Last Spells,
    pauses, time orbs, human rate; `Game::rec_cards`; cfg copy). Saved on Result removal to the port's save dir, loaded at
    start (HUD hiscore uses it). Session column 31 (result state) is now compared.
  - Verified: headless test `result::tests::game_over_name_entry_saves` (stage 1 → game over → No → name "PP" → panel →
    title → score.dat reloaded from the save dir has the entry and LSNM, save → load → save byte-identical with the same
    RNG; then the viewer ranking shows it; `TH08_RESULT_SHOTS=k,…` writes screenshots). Draw logs: menu_mopup1 1593 →
    2709 identical frames, 84.74 → 99.77 %; menu_base 1754 → 1934, 70.81 → 73.94 %; menu_clr and demos unchanged;
    sessions golden 8055 / fullrun 6265 / selftest1 1422 unchanged. Thresholds raised.
  - SPEC-GAP 31 → 40 (result screen details). Questions: `docs/briefs/IMPL23_QUESTIONS.md`.

- impl24 (comment pass batch A, 2026-10-19): `ecl.rs`, `eclvm.rs`, `enemy.rs`, `bullet.rs`, `item.rs`, `rng.rs` now have
  `//!` module lines and spec references (07 / 08 / 11 / 13 sections, original names and addresses, offsets, units) on
  fields, constants, opcode handlers and kept quirks. Comments only: the session traces golden / fullrun_noclip /
  selftest1 are byte-identical (8055 / 6265 / 1422), demo MIN_SYNC 4450/3869/2937/1605 and the draw-log percentages are
  unchanged, and all 78 tests pass. Spec items that are still missing are marked "Not implemented". SPEC-GAP 40 → 53:
  code vs spec differences found in this pass, plus code with no spec section. Questions:
  `docs/briefs/IMPL24_QUESTIONS.md`.

- impl25 (comment pass batch B, 2026-10-19): `player.rs`, `pshot.rs`, `sht.rs`, `bomb.rs`, `effect.rs`, `gamemgr.rs`,
  `stage.rs`, `stdfile.rs` now have `//!` module lines and spec references (06 / 09 / 11 / 12 / 13 / 14 sections,
  original names and addresses, struct offsets, units) on fields, constants, functions and kept quirks. Comments only:
  with comments and whitespace stripped, all 8 files are token-identical to before. The full `cargo test` log matches
  the pre-change run apart from timings: golden / fullrun_noclip / selftest1 8055 / 6265 / 1422, demo MIN_SYNC
  4450/3869/2937/1605, all 78 tests pass, the wasm check passes. SPEC-GAP 53 → 93: code vs spec differences found in
  this pass, plus code with no spec section. Two old gaps were closed by the current specs (12 §5.9, 09 §3).
  Questions: `docs/briefs/IMPL25_QUESTIONS.md`.
- impl26 (comment pass batch C, 2026-10-20): `anm.rs`, `anmvm.rs`, `anmmgr.rs`, `gfx.rs`, `msg.rs`, `hud.rs`, `gui.rs`,
  `ascii.rs`, `text.rs` now have `//!` module lines and spec references (03 / 05 / 06 / 10 / 12 / 13 sections, original
  names and addresses, struct offsets, units) on fields, constants, opcode handlers, functions and kept quirks; stale
  `answers_*` citations replaced by numbered sections. Comments only: with comments and whitespace stripped, all 9 files
  are token-identical to before. golden / fullrun_noclip / selftest1 8055 / 6265 / 1422, demo MIN_SYNC
  4450/3869/2937/1605, drawlog demo 0 90.61 %, all 78 tests pass, the wasm check passes. SPEC-GAP 93 → 110.
  Questions: `docs/briefs/IMPL26_QUESTIONS.md`.
- impl27 (comment pass batch D, 2026-10-20): `menu.rs`, `menulist.rs`, `result.rs`, `supervisor.rs`, `replay.rs`,
  `scoredat.rs`, `config.rs`, `archive.rs`, `input.rs`, `audio.rs`, `sound.rs`, `task.rs`, `game.rs`, `app.rs`,
  `assets.rs` now have `//!` module lines and spec references (00 / 01 / 02 / 11 / 12 / 13 / 15 / 16 sections,
  original names and addresses, offsets, units) on fields, constants, functions and kept quirks; the tools
  (`session`, `demosync`, `drawlog`, `drawreport`, `anmdebug`, `env`, `lib`, `main`, `bin/*`) have `//!` lines saying
  what they do. Comments only: with comments and whitespace stripped, all of `src/` is token-identical to before.
  golden / fullrun_noclip / selftest1 8055 / 6265 / 1422, demo MIN_SYNC 4450/3869/2937/1605, draw logs unchanged
  (demo 0 90.61 %), all 78 tests pass, the wasm check passes. SPEC-GAP 110 → 172. Questions:
  `docs/briefs/IMPL27_QUESTIONS.md`.

- impl28 (16 §1.1–§1.7, §3, 2026-10-20): `src/ending.rs` replaces the ending stub (scene 9). Ending object (calc 5,
  draw 4): Ending_Init clear records (bad: CLRD+0x20 read and cleared, music 18; good: Final A/B bit 0x8000/0x4000 in
  the no-continue / any masks of the shot and entry 12, clears counter, skip-allowed from the earlier masks,
  "skip on the next file" from Extra unlocked / any Final A, music 18/19), `staff01.anm` in slot 24, 15 text VMs
  (text.anm scripts 0x12+i at (64, 400 + 16i)), file choice by team (`end0N{a,b,c}.end`). `.end` interpreter: page /
  line timers with min-waits and Shot/Enter/Ctrl release, all commands (`@0`–`@3` fades, `@F` with the `@R`
  fall-through, `@M`, `@V`/`@v` picture scroll, `@a` staff VMs, `@b` picture, `@c` colour 0xRRGGBB, `@m` BGM + music
  flag 0, `@r`, `@s`, `@w`, `@z`), Ctrl fast-forward ×9 when skip-allowed. Draw: picture source rect (x, y) 640×480,
  text VMs, fade overlay stepped per draw. Text lines use Text_RenderToVm with outline −1 (±1 px, new
  `Raster::render_outline`); `Gfx::image_rect` copies a source rectangle. Removal → scene 6 (result name entry) → title.
  Music room done (impl70, `src/musicroom.rs`). Unlock notices (16 §4) not done.
- Debug: `TH08_ENDING=<shot 0–11><a|b|c>` (bad / good 6A / good 6B) or `TH08_ENDING=<file>.end` starts that ending from
  boot (`TH08_RANK`, `TH08_CONTINUES`, `TH08_PLAYER` for the file form). The scene-switch log line shows the calc frame.
- Verified by screenshots (bad end00a pages, good end00b / end03c, staff roll start / credits / end, result panel at
  100 %, title fade-in) and a scripted stage 1 → 6A → ending → staff roll → result → menu run. Metrics unchanged
  (see the impl28 report). Questions: `docs/briefs/IMPL28_QUESTIONS.md`.

- impl29 (RE-Q24 sync bugs, 2026-10-20): capture time reward as time orbs from the capture ring (07 §11.1:
  `EnemyManager::capture_ring_step` in `Spell_Calc`, centre follows the player, min(r,7) + 6 type-10 spawns per frame
  from t > 8, payout at ring t 130 with AddTime/max point only for a remainder; `Spell_Start` payout adds the time
  without max point); death shrink timer counts from the hit (09 §6.2; not reset at the real death, last vx/vy := 0
  each shrink frame); respawn clear +0xE2A70 := 60 every respawn call and counted down in UpdateInvuln (09 §6.3);
  op 67/178 y-edge tests sequential (07 §8.3).
  - Leading rows: golden 8055 → 8104, fullrun_noclip 6265 → 7552, selftest1 1422. Demo sync 4450/3869/2937/1605 →
    4450/3869/3081/1605. Draw logs unchanged. fullrun smoke now reaches 5 stages (desynced input; assert lowered).
    SPEC-GAP 179 → 182. Questions: `docs/briefs/IMPL29_QUESTIONS.md`.

- impl30 (RE-Q25 draw fixes, 2026-10-20): boss "ENEMY" markers (12 §5.10) in `src/ascii.rs`: four ASCII VMs
  (`ascii.anm` script 10) restarted by `gauge_setup`, run every `Ascii_Calc`, frame counter +0x8284; interrupts from
  op 127 (1 / 2), `Enemy_Free` of a boss (2 + pos −999), Player_Load (2 to M_0–M_2), full-exit player on-remove
  (99); `EnemyMgr_Calc` writes pos (x + 32, 472) and the mode (warning level + 1 / hit flash) after the hit flash;
  drawn at the end of the prio-20 ASCII pass (alpha by player distance, 0x80FF4040 flash, sprite 158 blink).
  Hit flash + damage sfx now on any d > 0 at step 6, also without flags A bit 3 (07 §10.1 step 7).
  Draw mode 5 uses the projected z per vertex; mode 4 = DrawBillboard via `AnmManager::cam_right` (05 §9;
  unreachable through the dispatch). Msg op 14 = ScreenEffect type 4, 442 frames, prio 0x15 (13 §3); HUD flash removed.
  - Golden markers match the reference draw log on every synced frame (positions, alpha, flash) except the blink
    phase (one frame, SPEC-GAP Q1). Golden draw log 25.10 → 25.14 %. Leading rows 8104 / 7552 / 1422, demo
    MIN_SYNC 4450/3869/3081/1605, demo / menu draw logs unchanged, 81 tests, wasm check passes. SPEC-GAP 182 → 180.
    Questions: `docs/briefs/IMPL30_QUESTIONS.md`.

- impl31 (port infrastructure, no spec section, 2026-10-20): `src/settings.rs` — one typed `Settings` read once
  (`settings::get()`, also `Ctx::settings`), described by one data table `ENTRIES` (key, env name, kind bool / int
  range / enum choices / text / path, default, group Game data / Display / Start game / Cheats/debug /
  Recording/tools, one-line help). Sources: command line > env vars (web: URL params, same lowercase names) >
  `port.cfg` > defaults; CLI parser, env/URL reader, `--help` and the port.cfg reader/writer all run off the table;
  one boolean rule (1/0, true/false, on/off; `--x` alone = on, empty = on) and one error style (logged, value
  ignored). `src/env.rs` removed; every `TH08_*` switch keeps its name. `settings::test_game_dir()` replaces the
  per-test `TH08_DIR` helpers (`archive::test_archive()` now panics if the dir is set but th08.dat is unreadable).
  `src/launcher.rs` (native): game dir = CLI arg → TH08_DIR → port.cfg `game_dir` → `rfd` folder dialog (xdg-portal,
  no GTK); th08.dat checked case-insensitively, error dialog + retry, cancel exits 0, an invalid dir given on the
  command line / TH08_DIR exits 1; a picked folder is saved. port.cfg at `$XDG_CONFIG_HOME/th08` /
  `%APPDATA%\th08` / `~/Library/Application Support/th08` (also the default save dir now, so Windows/macOS saves move
  there from next to the binary). Display: `scale` 1–4, `fullscreen` (F11 or Alt+Enter toggles + saves; the Enter press is
  not passed to the game), `vsync` (AutoVsync / AutoNoVsync); web ignores scale/fullscreen. Game files are looked up
  case-insensitively natively too (`assets::find_file`).
  - Behaviour unchanged: golden trace and draw log byte-identical (8104, 25.14 %), fullrun_noclip 7552, selftest1
    1422, demo MIN_SYNC 4450/3869/3081/1605, demo / menu draw logs unchanged, 85 tests (4 new settings tests), wasm
    check passes. Changed meanings: `TH08_SE_LOG=0` / `TH08_BDUMP=0` / `TH08_IDUMP=0` / `TH08_SESSION_PB=0` /
    `SHT_DUMP=0` now mean off (were "set = on"); a bad `TH08_STAGE` / `TH08_PLAYER` / `TH08_ENDING` is logged and
    ignored instead of failing start-up.
- impl32 (port infrastructure, no spec section, 2026-10-20): launcher screen (`src/launcher.rs`, egui 0.37 +
  egui-wgpu 0.37 + egui-winit 0.37, exact matches for wgpu 30 / winit 0.30.13) drawn on the game's own wgpu device
  and window before `settings::init`. Built only from `settings::ENTRIES`: one collapsible section per `Group` (Game
  data, Display and any group holding a CLI/env value open), widget per `Kind` (bool checkbox, int slider ≤16 values
  else drag value, optional ints with an enable box, enum combo with "(not set)", text with the format hint, path /
  new `Kind::File` with rfd Browse…), help line per row, "set by TH08_X / --x / URL ?x=", "port.cfg", "changed"
  markers, inline validation (`Entry::check`). Game folder row on top (Browse…, th08.dat check; web: the files picked
  in the page); Play disabled while the folder or a value is invalid. Play: `Model::save` writes only rows edited on
  screen (removed when back at the default; comments/other lines kept; `settings::write_port_cfg`), builds the final
  `Settings` (`Settings::from_values`), drops egui (renderer + GPU objects), applies vsync (`Gfx::set_vsync`),
  fullscreen and scale, then `settings::init` and `Ctx::load` on the same device — the game path is unchanged.
  New setting `launcher` (Display, `TH08_LAUNCHER`, default on); `--no-launcher` / `launcher = off` start directly
  (impl31 flow); tool runs (`Settings::is_tool_run`: shot, drawlog, input, anm, slot_test) never show it.
  Web: same screen in the canvas after the JS folder step; `?launcher=0` skips; the port.cfg slot is localStorage
  (`th08.<key>`). CLI now parsed by clap 4 (builder API generated from `ENTRIES`, native only): `--key`, `--k_e_y`
  and the URL name as aliases, `--no-<bool>`, `--bool[=on|off]` (and `--bool on`, joined before clap), per-entry
  value validation through the table parser (ranges, choices, aliases), `--help` grouped by `Group` with env name
  and default; unknown flags / missing or invalid values exit 2 with clap's error; env/port.cfg keep log-and-ignore.
  Tool `launchershot OUT.png [WxH] [switches]` renders the screen offscreen (`launcher::render_image`).
- impl45 (RE-Q28 CODE BUGs 9–11, cloud, 2026-10-20): `EnemyManager::step` returns right after the play-time counter
  while GameFlags bit 10 (no timelines, slot loop, draw-list rebuild, manager clock, card timer; 07 §10.1 step 0);
  the 0x164D30C play-time counter moved from `GameMgr_Calc` to the top of the enemy step, outside dialogue (11 §2).
  `GameMgr_OnRemove` already zeroed 0x164D2C8 as its last statement (11 §2), comment only. Exit save: the Supervisor
  only returns quit on requested −1; `Ctx::exit_save` flushes score.dat after the quitting frame's draw and Present
  (app) and after its trace sample (session) (15 §2.2). Traces not run (cloud). SPEC-GAP 134 → 133. Questions:
  `docs/briefs/IMPL45_QUESTIONS.md`.
- impl44 (RE-Q28 CODE BUGs 6–8, cloud, 2026-10-20): `ItemMgr_DropAttracted` (state-1 items → state 0, vel (0, −0.9, 0))
  as the last step of `Player_Die` (both modes) and after the real-death drops (09 §6.1–6.2); single-kill gauge
  through `GaugeAdd(−gauge/12)`, skipped while bombing, clamped (07 §10.3a); graze: the third time orb only inside
  the bullet-and-card branch (09 §9.2). Traces not run (cloud). SPEC-GAP 148 → 147. Questions:
  `docs/briefs/IMPL44_QUESTIONS.md`.
- impl43 (RE-Q28 CODE BUGs 3–5, 2026-10-21): chained damage forward to the root (07 §10.1 step 6): skipped while
  bombing, rd = d/2 then rd/9 (boss) or 0 by the root's damage timer, floor = max(0, the root's thresholds), applied
  only when rd ≠ 0. Death sfx, sparks and time orb for every death mode (also 3 and 4–7), both spark spawns only when
  e+0x3310 ≥ 0 (07 §10.3). Burst effects 21/26: random angle FloatRange(2π) − π only when param.x ≤ −990, else
  `Angle_AddNormalize(param.x)` (13 §2.6). Sync: golden 9046 → 10781, fullrun_noclip 9345 → 9407 (`MIN_LEADING`),
  selftest1 1921, demo MIN_SYNC 4628/3879/4035/6664 unchanged; draw logs: golden 38.17 → 47.51 %, selftest1 95.74 %.
  SPEC-GAP 167. Questions: `docs/briefs/IMPL43_QUESTIONS.md`.
- impl42 (RE-Q28 CODE BUGs 1–2 + port infrastructure, 2026-10-21): tests, `settings::get()`/`test_game_dir()` and every
  `src/bin/*` tool use defaults + env/CLI only (`Settings::from_environment` / `from_args`); only the game entry
  points read port.cfg / localStorage (`Settings::game_from_args` in `main.rs`, `Settings::game_environment` in the
  web `start`); test `tests/port_cfg.rs`. Var 10096 (07 §6): a child is evaluated on its parent (0 unless the parent is
  a root). `EnemyManager::take` keeps the running enemy's chain links in its placeholder and `put` copies the slot's
  links back, so walks pass through the running enemy (07 §6). Sync: golden 8297 → 9046, fullrun_noclip 7745 → 9345
  (`MIN_LEADING`), selftest1 1921, demo MIN_SYNC 4628/3879/4035/6664 unchanged; draw logs: golden 26.94 → 38.17 %,
  selftest1 95.74 %. Identical with and without a port.cfg holding `invuln = on`. SPEC-GAP 168 → 167.
  Questions: `docs/briefs/IMPL42_QUESTIONS.md`.
- impl40 (RE-Q25 CODE BUGs 7–9, 2026-10-20): texture capture `Gfx::capture_rect` (Anm_CaptureToTexture: filtered copy of
  the last presented frame's playfield rect into capture.anm#0 at the capture VM's sprite rect, logged as `K screen`);
  pause and continue menus start a capture VM (capture.anm script 0 at (32, 16), SPEC-GAP), request the capture on open
  and draw in the playfield viewport: capture VM (pause: copy with z-write off while state ≠ 0) then the menu VMs, all
  Draw2DSnap; continue clock VM only in a normal game below Extra (12 §3, §4). Difficulty sprite 0x120 only for
  spell-practice ids ≥ 205, else 0x11B + difficulty, shared by the pause menu and the HUD (`ascii::difficulty_sprite`).
  Continue "Yes" default only in spell practice with spell flag bit 9 clear (`enemy::Globals::spell_captured`, set on
  capture, cleared at Spell_Start, SPEC-GAP). Gui player show with face −1 restarts strips 2/4 (12 §5.8). Night clock:
  Hud_Load alpha 0, ECL 180 alpha := 0, player-overlap fade (−4 to 0x40 at x ≥ 64, y < 128, else +4 to 0xFF), drawn
  Draw2DRot after the title VMs and the stage-start clock (12 §5.7). Sync: golden 8297, fullrun_noclip 7745,
  selftest1 1921, demo MIN_SYNC 4628/3879/4035/6664 unchanged; draw logs: selftest1 95.49 → 95.74 %, golden 26.94 %,
  demo / menu draw logs unchanged. SPEC-GAP 169 → 168. Questions: `docs/briefs/IMPL40_QUESTIONS.md`.
- impl52 (audit A.17, A.18, cloud, 2026-10-21): enemy after-image trail (07 §10.1 step 4, §10.6): 96-entry ring
  `Enemy::trail_ring` (template x = −999) shifted on every step-4 pass after movement while e+0x534C ≠ 0; the screen
  check frees a trail enemy only when the oldest entry n − 1 is off screen too; the trail is drawn before the main VM
  (sprite copies oldest first with flags 2/4 scale/alpha fade; flag 8 strip via `AnmManager::draw_ring_strip`,
  unreachable with retail data). Op 157 stores u8 / i16 fields. Op 176 bit 13: `last_spell_box` calls
  `Player_DamageAgainst` over the whole playfield with boss 0's accumulator each frame, damage discarded (07 §7).
  Unit tests for the ring, the oldest-entry test, mid() and the box. Traces not run (cloud). SPEC-GAP 99 → 99.
  Questions: `docs/briefs/IMPL52_QUESTIONS.md`.
- impl62 (audit A.41, A.42, A.43, cloud, 2026-10-20): screen effects 1/2/4/7 follow 13 §3: shakes 1/7 hold on GameFlags
  bit 10 or the freeze byte, otherwise tick first and end before the RNG draws; type 1 and fades 2/4 are removed while
  0x18B8A2C ≠ 0 (now a global `enemy::load_frames`: native 30, StageStart 2, GM on-remove 1, Result/MainMenu 0,
  Supervisor decrement); type 7 is removed while the gameplay frame counter < 2. Ring init 0x4272E0: 24 segments,
  crossed param copy (13 §2.6). List C draws as `DrawBillboard` through `Camera_Set2D` on the playfield (13 §2.4).
  Unit test for the ring init. Traces not run (cloud). SPEC-GAP 148 → 145. Questions: `docs/briefs/IMPL62_QUESTIONS.md`.
- impl61 (audit A.40, A.44, A.47, cloud, 2026-10-20): the HSCR human-rate counters (15 §2.3) moved from the player to
  the top of `EnemyMgr_Calc`, in the play-time block (outside dialogue, also during the bit-10 freeze), counted only
  while the manager clock is > 15; human = player byte +3 (`focus`) == 0. StageStart registers Effects before HUD
  (11 §3.1) and only loads BGM (`Audio::load_bgm` = `Music_Play`, 02 §4): slot 0 and slots 1/2 whose STD name does not
  start with ' ' (spell practice: slot 0 from table 0x4C7670 by card id); slot 0 is played by the deferred music start
  in `GameMgr_Calc` step 3 (`Audio::play_slot`); MSG op 7 plays the preloaded slot n. New `Game::cfg_game` = the
  60-byte gameplay copy `*(GM+4)`: set at a fresh start (11 §3.2: lives 2/8 for Extra/practice, slow 0 for
  Extra/practice/spell practice) and overwritten with all 60 replay bytes by `play_stage_load` (11 §3.4); lives
  (fresh start, continue, HUD penalty), the PLST play-count test and the replay record/playback slow tests read it.
  The Supervisor config `cfg` is no longer touched by playback (`cfg_live` removed), so SE, volumes and the effect
  level (13 §2 reads 0x17CE894) stay the player's. Unit tests: human rate, config copy, spell BGM table. Traces not
  run (cloud). SPEC-GAP 115 → 114. Questions: `docs/briefs/IMPL61_QUESTIONS.md`.
- impl66 (audit A.62, A.63, A.64, A.67, cloud, 2026-10-20): `msg::face_files` follows the 10 §3 tables (stage faces
  per index; in spell practice no player faces and slot 18 = the `…sp` cut-in chosen by card id, Ex: none outside its
  card ranges). `Ending_Init` step 1 resets the AnmManager texture caches and runs `Ctx::clear_present_twice` (two
  white clear+Presents, 16 §1.2); Ending_Ctor start values text colour 0, +0x2A94 = 0, and LoadScript sets only
  +0x2A90 := 8. `ScoreDat::rebuild_lists` re-inserts the stored HSCR entries with ScoreList_Insert at every
  Result_OnRegister (modes 0/1 in `on_register`, mode 2 in `save_score(flush)`), so equal scores flip per load/save
  (15 §2.3); `ranking` keeps the in-memory order and puts entries above equal default rows. Unit tests for all four.
  Traces not run (cloud). SPEC-GAP 134 → 129. Questions: `docs/briefs/IMPL66_QUESTIONS.md`.
- impl67 (audit §B th08.cfg, frame skip, cloud, 2026-10-21): `Config` keeps every th08.cfg field (pad buttons, stick
  thresholds, frame skip +0x23, ShotSlow +0x26, debug flags +0x38 with bit 0 forced, padding and unused bytes), so
  `to_bytes` writes the file back as loaded. `Config::load` (15 §1.2, 02 §1) probes the thbgm.dat header (bad header =
  startup error; missing + defaults = music 2); `Ctx::load_with` reads th08.cfg (port save, else the game folder),
  writes it back at once and again in `exit_save` (native: next to score.dat; web: localStorage). `Ctx::draw` runs
  the draw chain only when the i8 skip counter reaches cfg +0x23 (00 §2). Supervisor +0x154 4000-frame check: not in
  the spec (question). Unit tests: round trip, load rules. Traces not run (cloud). SPEC-GAP 99 → 95 (`grep -c`
  lines). Questions: `docs/briefs/IMPL67_QUESTIONS.md`.
- impl60 (audit A.37, A.38, A.39, cloud, 2026-10-20): the stage-clear frame sets loading := 1 and score_display :=
  score for every stage; practice, Extra, 6A/6B and the night-over ending return 3 before the confirm test, which
  then runs on the next frame; the old GM's later calcs only count loading frames (no loader behind them) until the
  scene switch (`GameManager::stage_end`, 11 §4 step 1). RANK_STATE boots as 0, 0, 0, 0 (11 §2). A.39 was already
  fixed by impl45 (`EnemyMgr_Calc` counts 0x164D30C outside dialogue); the dead `Game::play_time` duplicate is gone.
  Unit tests for the clear-frame paths and the boot rank. Traces not run (cloud). SPEC-GAP 115 → 114.
  Questions: `docs/briefs/IMPL60_QUESTIONS.md`.
- impl48 (audit A.50, A.51, A.52, cloud, 2026-10-20): BGM volume uses the square curve `config::bgm_volume_curve`
  (02 §4 `0x472840`; the cube stays for SE); `Audio::fade_bgm` queues command 5 with trunc(secs), started in
  Sound_Update (`start_fade`) and ticked at the top of the Supervisor calc (`tick_fade`): p = n·5000/N − 5000, linear
  in dB to −50 dB over trunc(secs)·60 frames, then stop (02 §4); main-menu Quit no longer fades WAV music (12 §2.2).
  Unit tests for both curves and the fade. Traces not run (cloud). SPEC-GAP 148 → 148 (2 removed, 2 added).
  Questions: `docs/briefs/IMPL48_QUESTIONS.md`.
- impl49 (audit A.20, A.55, A.65, cloud, 2026-10-20): laser cancels set width := current width so the fade shrinks
  from the current size (`cancel_lasers`, 08 §7); key repeat tests the unchanged-input counter before incrementing it,
  first repeat on the 33rd equal frame (`Input::tick`, 11 §7); `Archive::from_bytes` rejects count ≤ 0 or table offset
  ≥ file size (01 §4) and `Archive::read` returns data without `edz` + known type char as-is (01 §6 step 4); the block
  cipher deciphers nothing when the tail exceeds a short odd input (01 §2, signed n) instead of panicking. Unit tests
  for repeat frames and synthetic archives. Traces not run (cloud). SPEC-GAP 148 → 144. Questions:
  `docs/briefs/IMPL49_QUESTIONS.md`.
- impl47 (audit A.45, A.60, A.61, cloud, 2026-10-20): `ScoreDat::insert_hscr` puts a new entry before equal scores
  (15 §2.3); name-entry Up, Down, Left, Right are four independent tests per frame in that order (`move_char_cursor`,
  16 §2.2); the HSCR / replay date is local time (`chrono::Local` native, JS `Date` on wasm; 16 §2.2). Unit tests for
  ties and Up+Down. Traces not run (cloud). SPEC-GAP 168 → 167. Questions: `docs/briefs/IMPL47_QUESTIONS.md`.
- impl64 (audit A.53, A.54, A.59, cloud, 2026-10-20): option volumes (items 3/4) step ∓/±4 on a Left/Right edge
  then ∓/±1 on an edge or while held with the unchanged-input counter > 29 (`menu::volume_step`, 15 §1.3); SE 0x1D
  volume preview every 50 screen frames on items 3/4, no SE on a volume step; title music is skipped when the
  previous scene was 5 (`menu::title_music`, 00 §3.2); resulttext.anm in ANM slot 22 (00 §3.2). Unit tests for the
  volume steps and the music rule. Traces not run (cloud). SPEC-GAP 134 → 131 (src). Questions:
  `docs/briefs/IMPL64_QUESTIONS.md`.
- impl65 (audit A.56, A.57, A.58, cloud, 2026-10-20): spell-practice confirm sets difficulty = `Spell_DifficultyOf(id)`,
  Last Words Normal + id stage (`menulist::spell_start`, 16 §6.2); a screen-16 shot change zeroes +0x1442C (pie easing
  restarts, input stays on) and Bomb stores the row into g_StageIdx; screen 16 entry sets the row from g_StageIdx
  (16 §6.1). Handlers that return 2 (9 → 11, 15 → 16, 11 → 9, 16 ↔ 17) set `MainMenu::again` and `calc` runs the
  handler, RunRange and the description VM again in the same frame (12 §2); the mopup1-derived press-frame special
  cases are gone. Back from a practice game the menu starts on 8 / 15 with +0x6C (`MainMenu::back`) and forwards
  8 → 9 → 11 / 15 → 16 → 17 (12 §2.2); screen 15 is handled by `character`. Replay stage row: cursor on the first
  recorded stage at the file confirm, +0xC290 written only in sub-state 2 (15 §3.8). Unit tests for the confirm
  difficulty/stage and the replay stage cursor. Traces not run (cloud). SPEC-GAP 115 → 106 (src). Questions:
  `docs/briefs/IMPL65_QUESTIONS.md`.
- impl51 (audit A.13, A.14, A.66, cloud, 2026-10-20): `spawn_child` (ops 90–92) applies bit 8, bit 11, layer,
  contact off and the aura **after** the child's spawn-time `Ecl_Run` (07 §10.5); aura VM bit 17 := flags A bit 2 and
  VM +0x14 negated for an odd slot; the chain link (parent, tail +8, child +4, spawner +0x3380) is written last, with
  the tail looked up before the spawn: a root links after its last child, a chained child after itself (cutting the
  rest off). `spawn_prepare` writes only pos; prev pos / final pos keep the template's (0, 0, 0) (07 §4.1). Unit
  tests for all three. Traces not run (cloud). SPEC-GAP 115 → 115 (2 removed, 2 new). Questions:
  `docs/briefs/IMPL51_QUESTIONS.md`.
- impl50 (audit A.12, A.15, A.16, cloud, 2026-10-20): `EnemyManager::kill_all` (op 95, op 123 `Spell_End`, death
  modes 0–2) and `kill_non_bosses` (threshold / timeout switch) have no self test (07 §7 op 95, §10.2): the ECL
  caller is put back in its slot with its running main context (`Exec::with_self_in_slot`), so a non-boss caller
  without flags B bit 6 gets life 0 and its death sub in slot order (main context: the death sub runs at once; thread:
  the thread is freed). Native 31 drops item type 3 while the player is bombing, else 5 (07 §12); op 181 does nothing
  at all at clock 12, sfx 0x2D only below (07 §7). Unit tests for all three. Traces not run (cloud). SPEC-GAP
  134 → 130. Questions: `docs/briefs/IMPL50_QUESTIONS.md`.
- impl54 (audit A.22, A.23, A.26, cloud, 2026-10-20): damage areas test enemies per 09 §4.5 (`Area::hits_enemy`:
  circle = point test on the enemy centre `≤ r²`, rectangles = inclusive box overlap, unrotated box in the area frame);
  the bomb active flag +0xE2A7C is no longer cleared at the bomb end but by the next failed normal start test (09 §1.1);
  game over leaves lives and bombs unchanged (09 §6.2). Unit tests for all three. Traces not run (cloud).
  SPEC-GAP 134 → 132. Question: IMPL54_QUESTIONS.md.
- impl55 (audit A.24, A.25, A.27, cloud, 2026-10-20): `Player::frame` step 1: while the freeze byte 0x160F534 is set
  the youkai aura (fixed slot 2) and gauge-max aura (slot 8) get VM flag 0x80000 and the calc returns, else the flag is
  cleared (09 §1); the focus-switch +0xE2A9C := 0 now only on the team sprite reset (09 §3); spell practice (bit 14):
  load timer 10, respawn leaves the invulnerability timer at 30 (09 §6.3, §6.4). Unit tests for A.24 and A.27 (A.25
  needs ANM scripts). Traces not run (cloud). SPEC-GAP 115 → 111. No questions.
- impl59 (audit A.34, A.35, A.36, cloud, 2026-10-20): `Stage::step` returns at once while GameFlags bit 10 (player
  `freeze_bullets`): no script, camera, clock, sparkle flag or frame counter (06 §5); sparkles spawn from frame 0 in
  spell practice (GameFlags bit 14, `Stage::sparkle_due`, 06 §5); a billboard appends a sparkle point only when it is
  drawn (0 ≤ z ≤ 1), `v < fog_near` quirk in `sparkle_point_ok` (06 §11.1). Unit tests for all three. Traces not run
  (cloud). SPEC-GAP 148 → 145. No questions.
- impl58 (audit A.32, cloud, 2026-10-20): a player bullet entering its hit state follows the exact 09 §4.5 order:
  rotation z saved → `AnmFile_SetScript` (new `AnmManager::file_set_script`, VM pos / pos offset kept) → rotation z
  restored → effect 5 → pos z 0.1 → state 2; flag 0x2000 is no longer re-set (only the hit script sets it). Unit test.
  Traces not run (cloud). SPEC-GAP 115 → 114. No questions.
- impl57 (audit A.29, A.30, A.31, cloud, 2026-10-20): Yukari's option always runs the animation test after the attack
  test, on the sub-state the frame entered with, so it can overwrite sub 3 / interrupt 3 while the aimed pointer stays
  (14 §3.1); a shot origin reads the option pos whatever the option's state (09 §4.3); laser init 4 and update 5 both
  honour GameFlags bit 13 (refuse / end the laser, 14 §4). Unit tests for all three. Traces not run (cloud).
  SPEC-GAP 134 → 130. No questions.
- impl56 (audit A.28, A.33, A.68, cloud, 2026-10-21): `Player_Load` puts the player at (192, 384, 0.49) and the
  respawn point is (192, 384, 0.2) (09 §6.4); the dead `box_in_cancel_area` is gone and `Area::contains` is the plain
  point test (09 §8); the +0xE2A6C cooldown drops at the top of `update_bomb`, skipped on a forced-start call
  (09 §1.1). Unit tests for the start/respawn z and the cooldown. Traces not run (cloud). SPEC-GAP 99 → 96. No questions.
- impl39 (RE-Q25 CODE BUGs 4–6, 2026-10-20): screen effects registered from calc nodes below prio 3 are linked at once
  (`effect::screen_now` for the `GameMgr_Calc` demo-end fade, `loading_overlay` from the Supervisor on-register), so
  their first calc runs in the same frame (13 §3). Stage-clear panel per the rewritten 12 §5.6: op 9 stores the values
  and restarts / clears the stage-clear VM (sprite 0x80 + clock advance, 10 §9); `Hud_Calc` computes the bonus once on
  the next frame in i32 (lives/bombs bonus unless practice, 6B night bonus always, integer rank multipliers, lives-option
  penalty always); the draw builds the strings each frame with the table's colours and pen layout, lines hidden for
  practice and practice replays (`Game::practice_replay`, info +0x7B), integer percentages, "PM%2d:%02d" clock line at
  x 120 / 219 / 253. HUD draw order clear panel → Msg → boss UI / title / clocks / stage-clear VM (playfield viewport)
  → panel → popups (12 §5.3); ASCII strings carry the clip flag (12 §2.9), popups use it; spell-bonus lines at
  (105, 80) and ((384 − len·28)/2 + 32, 96) (12 §5.5). Sync: golden 8297, fullrun_noclip 7745, selftest1 1921, demo
  MIN_SYNC 4628/3879/4035/6664 unchanged; golden draw log same quads 2165520 → 2176755 (26.80 → 26.94 %); selftest1
  95.49 %, demo / menu draw logs unchanged. SPEC-GAP 174 → 169. Questions: `docs/briefs/IMPL39_QUESTIONS.md`.
- impl38 (RE-Q27 CODE BUGs 8–9, 2026-10-20): `Spell_Calc` card-active steps 1.1–1.7 (07 §11.1) in
  `EnemyManager::spell_step` + `Gui::calc`: the card ends silently (bit 0 off, `Spell_HideName`, no Spell_End) when the
  boss is dead or its slot index differs from the one stored at Spell_Start; bonus-line tint +0x2620 (reset at
  Spell_Start) alpha fades 0x80 ↔ 0x20 by 4 per frame with the player in the top band x ≥ 64, y < 64 (12 §5.8);
  failed-ring look (sprite 0xDD → 0xDE, scale.x / scale_end.x := 4) once capture is lost; wavy start ring (+0x32C :=
  VM pos.y while waves ≠ 0, then plain); shrink 256 → 8 over duration − 100 once the grow-in slot ends; rotation via
  `Angle_AddNormalize` (+π/200, −π/100 for a last spell). Spell_Start ring init per 07 §11.1 (pos slot (8, 64) →
  (256, 0), radius 256, width 15, waves 6, rotation 0). Spell_End: capture restyle (pos slot → (256, 0) over 30 mode
  6, colour slot → 0x20D080A0 over 60 mode 3, waves 6); not captured → ring freed. Capture ring: t 30 / 60 restyle
  slot writes, t 130 payout first and nothing else that frame, exact rotation constants via `Angle_AddNormalize`,
  +0x32C := VM pos.y, orb angle from `Timer_GetF`. Sync: golden 8297, fullrun_noclip 7745, selftest1 1921, demo
  MIN_SYNC 4628/3879/4035/6664 unchanged; golden draw log same quads 2088144 → 2165520 (25.84 → 26.80 %); selftest1
  95.49 %, demo / menu draw logs unchanged. SPEC-GAP 175 → 174. Questions: `docs/briefs/IMPL38_QUESTIONS.md`.
- impl37 (RE-Q27 CODE BUGs 7, 10 + selftest1 1540, 2026-10-20): gameplay frame counter 0x164D2C8 := 0 in
  `GameManager::register` and `on_remove`, so the menus after a pause-quit read 0 (11 §2); `Ascii_Calc` does not run
  during the boot calc passes (scene 0), so +0x8284 equals the main-menu frame number (12 §5.10); death mode 0 keeps
  the boss table entry and flags A bit 1 (`release` no longer unregisters; `free` does), so the ENEMY marker stays at
  its last position (07 §10.3 rows 0–1, 12 §5.10); mode 3 turns the HUD boss display off for every enemy (07 §10.3).
  Sync: selftest1 1539 → 1921, golden 8297 / fullrun_noclip 7745 / demo MIN_SYNC 4628/3879/4035/6664 unchanged;
  golden draw log same quads 2087684 → 2088144 (ascii.anm +460, 25.84 %); demo/menu draw logs unchanged.
  SPEC-GAP 177 → 175. Questions: `docs/briefs/IMPL37_QUESTIONS.md`.
- impl36 (RE-Q27 CODE BUGs 4–6, 2026-10-20): `Stats::add_time` adds n to the parity +0x44 also for negative n
  (unclamped only; 11 §1.5 setters); Remilia team/solo options turn only while the option timer is > 12 (14 §3 table,
  §3.2); `GameMgr_Calc` step 7 recomputes GM flags bit 2 := paused == 0 and game-over == 0 before the step-8 pause
  return (11 §2.1, §4). Sync: selftest1 1422 → 1539, demo MIN_SYNC 4628/3869/3081/1605 → 4628/3879/4035/6664; golden
  8297, fullrun_noclip 7745, draw logs unchanged. SPEC-GAP 178 → 177.
- impl35 (RE-Q27 CODE BUGs 1–3, 2026-10-20): `Spell_Calc` card-active steps 1.3 / 1.6 (07 §11.1) in
  `EnemyManager::spell_step`, run from the Gui calc before the capture-ring step and the VMs (skipped while GameFlags
  bit 10 / freeze byte): per-frame bonus decay −= trunc((R/60)·gs) then rounded down to 10 while capture is possible
  (not bit 11 / boss flag A bit 27), start ring centre += (boss pos + offset − centre)/16 unless bit 6, z := 0.
  `Spell_Start` spawns the ring at the boss pos e+0x2D34 (type 52 while GameFlags bits 7–8), stores max +0x2638 and
  the boss slot, clears bits 6 / 11; `spell_add_bonus` (07 §11: R += n/120 below the max, else bonus := max, ignored
  while bit 11), one call per collected time orb; ops 164 / 184 set spell bits 6 / 11 (bit 11 cleared by Spell_End).
  `Player::in_bomb_area` no longer resets the cancel item; `graze_test` / `hit_test` / `contact_test` write 6 first
  (08 §4.1, 09 §8). Sync: golden 8104 → 8297, fullrun_noclip 7552 → 7745, demo 0 4451 → 4629 (thresholds raised).
- impl34 (port infrastructure, no spec section, 2026-10-20): release builds. `.github/workflows/release.yml` (tag `v*`
  or manual run): `windows` (windows-latest, MSVC, `+crt-static` via `.cargo/config.toml`, icon + version info via
  `build.rs`/winresource) uploads `th08.exe`; `linux` (container ubuntu:22.04, glibc 2.35) runs
  `packaging/linux/build-appimage.sh` (pinned appimagetool 1.9.1 + type2-runtime 20251108, sha256-checked) and uploads
  `th08-x86_64.AppImage`; on tags `release` (contents: write) creates/updates the GitHub Release with both files.
  The AppImage holds only the stripped binary (needs libasound, libc/libm/libgcc_s from the host; X11/Wayland,
  xkbcommon, Vulkan/GL are dlopen'ed from the host). Windows release builds use the GUI subsystem and attach to the
  parent console (`AttachConsole`) when started from a terminal, so `--help` and errors still print. Icon
  `packaging/th08.svg` (original artwork) rendered to `packaging/linux/th08.png` and `packaging/windows/th08.ico`.
- impl33 (port infrastructure, no spec section, 2026-10-20): generic code moved to crates. `src/xz.rs` (native) reads
  and writes `.xz` with `liblzma` 0.4.9 (static + parallel: liblzma is built from source, no system xz needed;
  multi-stream reader, preset 1 on all cores for writing), used by draw logs, session traces, `session` and `drawrun`.
  `settings::config_dir` uses `directories` 6 `BaseDirs::config_dir()/th08` (same paths as before).
  `audio::parse_wav` uses `hound` 3.5; `se_waves_parse` compares every SE sample with the old RIFF parser.

- impl53 (audit A.19, A.21, cloud, 2026-10-21): ECL native 26 sends interrupt 2 (arg ≠ 0) / 1 to the stage VMs;
  native 28 also scales live bullet velocities by 1/arg and maps sprites 0x60–0x6F → 0x6F (old sprite kept in the
  bullet's +0x218 copy `saved_sprite`, also set at spawn), interrupt 2; native 29 restores velocities and sprites,
  interrupt 1 (07 §12). `Spell_End` sets the bomb cooldown +0xE2A6C := 16 (07 §11.1); the decrement moved to the top of
  `update_bomb` and is skipped on a forced start (09 §1.1). Unit tests for the cooldown, `Spell_End` and the
  slow-down. Traces not run (cloud). SPEC-GAP 99 → 101 (src). Questions: `docs/briefs/IMPL53_QUESTIONS.md`.
- impl70 (audit §B): music room scene 8 (16 §5.1–§5.3) in `src/musicroom.rs`, replacing the Supervisor stub:
  musiccmt.txt parser (blank lines dropped, 7 comment lines, 32 entries), heard flags copied from PLST+0x208, title list
  (music00.anm slot 0x17, heard title / `？？？？？？？？` with their colours and outlines, 10-row window, numbers, cursor),
  state 0/1 calc (interrupts, Up/Down wrap, Shot/Enter `Music_PlayFile` + playing-title VM, Ctrl fade 8 s, R replay,
  Bomb/ESC → scene 1), comment reveal at sub-frames 10–22 with the warning table for unheard tracks. `text::Raster`
  takes an outline colour. Music mode cfg +0x1F = 0 (02 §4): `Audio` plays, loads and fades nothing; MIDI stays a
  SPEC-GAP. Unit tests for the parser, title / unlock texts, cursor window and music off. Traces not run (cloud).
  SPEC-GAP 99 → 111 (src). Questions: `docs/briefs/IMPL70_QUESTIONS.md`.

- impl76 (audit 78–80, cloud, 2026-10-21): `Timer::dec` slow path (speed ≤ 0.99) sets prev := cur, the fast path
  leaves it (05 §2); enemy step 6 skipped for flag A bit 31 while bombing (also the Yukari aimed pick), and the first
  `Player_DamageAgainst` call not made during a card for a parented enemy while bombing (07 §10.1 step 6); item
  collection compares f32 box edges against the item box stored by `Player_Move` (08 §8.4, `Player::item_box`).
  Unit tests for the timer and the collect test. Traces not run (cloud). SPEC-GAP 115 → 116 (src). Questions:
  `docs/briefs/IMPL76_QUESTIONS.md`.
- impl72 (audit §B, cloud, 2026-10-21): ECL natives 4 / 7 / 21 warp live bullets that leave the outer zone into the
  inner one (scaled about the shared centre, direction reversed, `BulletManager::warp`); 9 / 11 / 25 hit-test a
  590 × 128 / 192 / 224 box at the caller once its context clock reaches 12; 13 requests stage tint 0xFFC03030;
  16 sets var 10038 = 60 and the caller's var 10019 on chained children near a flag-0x100000 bullet (07 §12). The
  §12 table leaves the details open: SPEC-GAP 99 → 103 (src), questions `docs/briefs/IMPL72_QUESTIONS.md`. Unit tests
  for natives 4, 9, 13, 16. Traces not run (cloud).
- impl73 (audit §B, A.69–A.71, cloud, 2026-10-21): `Replay_FrameHook` (11 §1.4) at calc prio 7 while recording, and
  in playback when header +6 ≠ 0 (also called once on linking): +0xD8 := RNG state, `Rng::counter` := 0. The trace
  column `rng_ctr` now reads the never-reset `Rng::total`. `Replay_PlayCalcDebug` (15 §3.1) = PlayCalc reading only
  the input word, 3 words per step, no RNG forcing. Slow mode (11 §4 step 10) and playback slowdown (step 5) share
  the never-reset counter GM+0x3DE08 (`Game::skip_ctr`); step 5 is skipped while dialogue is active. gm+0x2D
  (`Game::slow_skip`) set by step 10, cleared by step 10, StageStart and the GM on-remove; while set the Supervisor
  OR-accumulates input (11 §7) and ESC does not pause. Sound flush (`Ctx::sound_update` in place) after SE 10 on a
  top-menu confirm (12 §2.2) and on Left in the card list (16 §6.2). Unit tests: hook, frame words, accumulation,
  step 5 / step 10 patterns. SPEC-GAP 115 → 111 (src). Traces not run (cloud). Questions
  `docs/briefs/IMPL73_QUESTIONS.md` answered by RE-Q29.
- impl69 (audit §B replay save, cloud, 2026-10-21): Result states 10–14 (16 §2.6): "save replay?" with the
  slow-mode / continue messages (interrupt from VM 4), slot list `replay/th8_01..15.rpy` (headers read at frame 0),
  overwrite prompt, name entry on the §2.2 grid writing +0x58 (pre-filled from LSNM), save → LSNM, draw of VMs 49–54
  and the slot rows. `replay::save` / `build_save` = `Replay_Finish` (15 §3.6): skip tests (changed config on a normal
  E–L game, recorded slow mode), name, slowdown +0x114/+0x118, card name +0x7E, USER section (15 §3.5); clear status
  +0x11C at record start; 0x164D0B0 (`Stats::d0b0`) recorded in +0x20 and restored on playback. `TH08_REPLAY_SAVE`
  still saves at the scene change when the dialogue did not. Unit tests: save → parse header fields, card name, skips.
  Traces not run (cloud). SPEC-GAP 99 → 98 (src). Questions: `docs/briefs/IMPL69_QUESTIONS.md`.
- impl71 (audit §B, cloud, 2026-10-21): D in the result top menu (sub-state 1) writes `score.txt` (16 §2.7,
  `result::score_txt`: Shift-JIS, CR LF, header + time stamp, per difficulty/shot HSCR blocks with captured cards,
  card table, play data with the correct continue total); native → port save dir next to score.dat, web →
  localStorage key `th08:score.txt` (same storage as score.dat). Hidden unlock code on the difficulty page (15 §2.12):
  Focus/Skip + Right ×4, Left, D ×2, Q ×3 → CLRD masks 0xFFFF, card attempts +1, SE 0x1C. Unlock notices (16 §4):
  MainMenu_OnRegister queues info00 / info02 / info01.jpg for flag bits 15 / 16 / 17 that turn on; `Ctx` runs the
  `Info_Show` loop (6000 frames, fades, skip from t 180 with SE 10) in place of the calc and draw chains; none at
  the first menu after boot. Unit tests: score.txt byte-identical to `specs/data/drawlog/menu_mopup1.score.txt`,
  code sequence + effect, notice order and fade bytes. Traces not run (cloud). SPEC-GAP 110 → 113 (src).
  Questions: `docs/briefs/IMPL71_QUESTIONS.md`.
- impl68 (audit §B, cloud, 2026-10-21): key config screen 3 (15 §1.3, `menu.rs` `key_config`): 12 rows in screen
  order (Skip 4th), working copy of the 9 pad fields with the live Up/Down at −1 while open, assigning from the first
  new raw pad button with swap + SE 10, ShotSlow Left/Right toggle, Reset (table 0x4C78C8, +0x26 := 1), Quit / 3600 idle
  frames write the copy back (SE 11, screen 2 cursor 8). Rows are printed with the ASCII queue (VM layout not in the
  spec). Joystick (15 §1.1): `input::joystick_bits` (pad fields → bits 0x1…0x100, lX/lY vs. +0x18/+0x1A, ShotSlow
  counter) ORed into the raw word; `src/joystick.rs` polls the first gilrs gamepad (browser Gamepad API on wasm; on
  Linux behind the `gamepad-linux` feature because gilrs needs libudev-dev). Option screen: cursor from the first call,
  Left/Right/confirm/cancel from frame 4, leaving = SE 11 / top cursor 7, snapshot taken by the top-menu switch,
  3600-frame idle exit, return 5 → `Game::device_reset` (the app applies +0x22 as fullscreen). Unit tests: mapping,
  ShotSlow counter, key swap. Traces not run (cloud). SPEC-GAP 111 → 113 (src). Questions:
  `docs/briefs/IMPL68_QUESTIONS.md`.
- impl75 (audit 75–77, cloud, 2026-10-21): rank sub-counter GM+0x3DE38 zeroed at every stage start after the
  fresh / continuing branch; `GameMgr_InitRank` (`init_rank`) writes rank / min / max only (11 §3.1, §3.2). GameFlags
  word (11 §2.1) is one thread-local in `gamemgr.rs` (`flags()` / `set_flags()`, `Game::flags` is a `GameFlags`
  handle on it); `Player::gflags` removed, bits 7 / 8 / 13 / 14 read and written there by ECL op 176, `Spell_End`,
  bombs, shots, the last-spell box and stage sparkles. Msg op 11 sets the clear trigger only in stage idx 6–8, else
  it only stalls (10 §5). Unit tests: rank init / sub reset, flags word, op 11 by stage. Traces not run (cloud).
  SPEC-GAP 111 → 111 (src). Questions: `docs/briefs/IMPL75_QUESTIONS.md`.
- impl77 (audit 81–82, cloud, 2026-10-21): bomb shakes take the EDX duration (14 header note): Sakuya (1: 120, 4, 1)
  every window frame, Sakuya LS (1: 50, 4, 1) at T = 0, Alice / Alice LS (1: 24, 8, 0) (§2.3, §2.4, §2.7, §2.8).
  Continue menu (12 §4): the player (prio 9) / enemy (prio 11, timeline op 16) calc writes `Game::game_over` itself
  (was relayed by the next `GameMgr_Calc`), so the ASCII calc opens the menu on the next frame; play time is added on
  menu entry (15 §2.9); the player sprite is hidden while game_over ≠ 0 (14 §5). Unit tests: shake arguments, menu
  first call. Traces not run (cloud). SPEC-GAP 115 → 115 (src). Questions: `docs/briefs/IMPL77_QUESTIONS.md`.
- impl74 (audit 72–74, cloud, 2026-10-21): `Hud::calc` follows 12 §5.1b — `Hud_Update` (boss bar; front, title,
  stage-start clock, stage-clear and night-clock VMs; night-clock fade; difficulty VM; popup timers; clear bonus;
  clear trigger + clock animation) first, then `Msg_Run`, then the frame counter, so op 9's bonus lands the next
  frame. Clear trigger (12 §5.6, `ClearPanel::step`): tested before the animation step, no 60-frame gate, impl+0x22E10
  delay timer. Clear panel: for stage idx > 6 the pen starts at y 80 for every line. Unit tests for the trigger and
  animation. Traces not run (cloud). SPEC-GAP 115 → 114 (src). Questions: `docs/briefs/IMPL74_QUESTIONS.md`.
- impl78 (user report, 2026-10-23): boss orbit squares (effect 13, ECL op 128, 13 §2.6b) drifted off the playfield:
  op 128's target radius d holds f32 bits (96.0 / 128.0) but was converted as an integer (≈ 1.1e9), so the radius
  grew forever. Now read as f32 bits (SPEC-GAP). Questions: `docs/briefs/IMPL78_QUESTIONS.md`.
- impl82 (audit 84, 97, 98): player shot vs enemy uses f32 box edges (helper 0x451CE0, min = c − s·0.5,
  max = s·0.5 + c, inclusive; 09 §4.5). The quitting frame runs calc + Sound_Update only, no draw and no Present
  (`Ctx::run_frame`, app loop; 15 §2.2, 00 §2). A forced bomb start jumps past the +0xE2A6C decrement, the bombing
  branch and the start tests (09 §1.1). Unit tests; traces not run (cloud). SPEC-GAP 112 → 112 (src). No questions.
- impl81 (audit 87, 88, 92, 95, 99, 100, cloud, 2026-10-23): `EnemyMgr_Calc` (07 §10.1 step 1) runs the op-176
  whole-playfield box, then `EnemyMgr_TimeRank`, then the list resets and timelines. Held enemies (step 2: flags B
  bit 7, or A bit 30 while bombing / player state ≠ 0) skip react, ECL, movement, screen check, VMs, collision and
  death; only the e+0x2E14 decrement and the step-7 tail run. `kill_all` (op 95): final pos recomputed, popups
  2000 then +30, items/popups also at every 6th trail entry, `Enemy_ChainUnlink` (original semantics, §10.3a: a
  root only clears its own links) on every victim, death sub via `Ecl_SetSub` only. Op 95 / 123 on the caller's
  own main context keep the local ip in the old sub (death sub consumed, never run); from a thread the threads are
  kept. Unit tests for all of it. Traces not run (cloud). SPEC-GAP 112 → 110 (src).
  Questions: `docs/briefs/IMPL81_QUESTIONS.md`.
- impl80 (audit 85, 90, 103, cloud, 2026-10-23): native 8 (07 §12) walks the parent's +8 chain with no alive test,
  writes int var 10037 per member, int vars 10005 / 10006 / 10007 on the caller, and adds the leader's orbit velocity
  when the 10007 counters differ. Natives 9 / 11 / 25 call the new `Player::laser_test` (`Player_LaserTest`, 09 §6.1,
  f32 box edges): origin = pos − (float vars 10016, 10017), main VM rotation z, small box with graze when e+0x2E14
  changed to a multiple of 12, big box every call, no warning phase. Natives 28 / 29: speed and interrupt first,
  29 scales by 1/speed and restores only sprites 0x60–0x6F. Unit tests for natives 8, 9, 11. Traces not run (cloud).
  SPEC-GAP 112 → 111 (src). Questions: `docs/briefs/IMPL80_QUESTIONS.md`.
- impl79 (audit 83, 89, 94, 104, cloud, 2026-10-23): `PlaceBullet` velocity = polar of the unwrapped pattern angle
  (08 §3.2); `CancelBonus` cancels every alive laser without the flag-bit-2 test and sets the cancel guard (08 §7);
  scripts 111 / 112 (types 16 / 17) → hitbox 4, class 4 (08 §2.1); bullet +0x218 at spawn = template base sprite
  (05 §2). Unit tests for each. Traces not run (cloud). SPEC-GAP 112 → 111 (src). No questions.
## Run
WebGPU in Playwright: headless Chromium only has SwiftShader and loses the device on the first canvas present (even a bare JS triangle); use a headed launch with `--enable-unsafe-webgpu --enable-features=Vulkan --ignore-gpu-blocklist --ozone-platform=x11 --force-device-scale-factor=1` (on Wayland at 1.25 scale `devicePixelContentBoxSize` makes the canvas 800x600 even with an emulated DPR of 1).
Web: `web/build.sh`, serve `web/dist` (e.g. `python3 -m http.server -d web/dist 8087`). Verified in Playwright Chromium (WebGL2 fallback, OPFS cache works).
Playwright can only upload from the repo dir: game files are hard-linked into `.playwright-mcp/game/`.
`cargo run --release --bin th08 -- "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night"` (or TH08_DIR env, or port.cfg `game_dir`, else a folder dialog; tests and tools never read port.cfg, only TH08_DIR); `th08 --help` lists every switch.

## Next (RE and IMPL pipelines run in parallel, one Opus agent each; IMPL uses specs/IMPL_TEMPLATE.md)
1. RE phase5 done (answers_phase5 + spec fixes). RE phase6-opus (running): 15 cfg/score.dat/replay, 16 endings/result/music room, answers_phase6.
2. IMPL impl22 done (boss phases, IMPL17 menu answers). Open: IMPL22 questions (op 36 a–d, payout time, fullrun 6266 homing hit, golden 8056, demo 2 2938). IMPL impl23 done (result screen, name entry, score.dat saving, viewer). Next IMPL: th08.cfg load/save, music room, info notices (16 §4), key config. Endings done (impl28). Launcher screen done (impl32).

## Final phase (after all specs are implemented and the game is complete)
Refactor to idiomatic Rust: the current code mirrors the spec structure (flat state, global-ish managers, raw offsets/indices).
Goals: clear module boundaries and ownership, typed enums/newtypes instead of magic ints, no unnecessary `unsafe`/`static mut`,
remove SPEC-GAP leftovers and dead paths, clippy-clean. Constraint: behaviour stays bit-exact — replays and the scripted
1→6 run must produce identical results before/after (use them as regression tests); keep the wasm build working.
