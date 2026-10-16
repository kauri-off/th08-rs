# Status

## Done
- `src/archive.rs` — PBGZ th08.dat reader (header/table cipher, LZSS, edz payload wrapper). All 317 files byte-identical to DeepSeek's extraction. `cargo run --release --bin datdump <th08.dat> <out>`.
- `src/audio.rs` — thbgm.fmt parse + cpal stream, linear resampling to device rate, intro then loop to `+0x18` (loop start/end VERIFIED by specs/opus/02_bgm.md). Device pick: pipewire → pulse → default (ALSA default is broken on this machine).
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

- `src/env.rs` — debug settings: env vars natively, URL query on the web (`?stage=4a&stage_frame=600`, `?anm=title01.anm:0-9`, `?anm_int=…`, `?slot_test=…`; key = env name without `TH08_`, lowercase). Web `?shot=<n>` freezes logic at frame n (native `TH08_SHOT` exits) so pages can be screenshotted.
- Web parity checked (2026-10-17): title (frame 120), `?anm=title01.anm:0-9`, stages 1/3/4a/6b (frame 600) are pixel-identical to native `TH08_SHOT` captures on WebGL2 (Playwright MCP Chromium) and WebGPU (headed Chromium, NVIDIA Vulkan). Title BGM starts after the first key press (AudioContext running). No backend changes were needed.
- Ground-tile seams on stages 1/3 investigated: geometry is watertight (shared edges project to bit-identical vertices), sampling stays inside the sprite rect (stage 1 row 339 reads texel row 1.52 of sprite (1,1) 254x254), clamp/point sampling don't change them, a white texture removes them → the sprite's first and last rows simply don't match in the data. Not a renderer bug.

## Run
WebGPU in Playwright: headless Chromium only has SwiftShader and loses the device on the first canvas present (even a bare JS triangle); use a headed launch with `--enable-unsafe-webgpu --enable-features=Vulkan --ignore-gpu-blocklist --ozone-platform=x11 --force-device-scale-factor=1` (on Wayland at 1.25 scale `devicePixelContentBoxSize` makes the canvas 800x600 even with an emulated DPR of 1).
Web: `web/build.sh`, serve `web/dist` (e.g. `python3 -m http.server -d web/dist 8087`). Verified in Playwright Chromium (WebGL2 fallback, OPFS cache works).
Playwright can only upload from the repo dir: game files are hard-linked into `.playwright-mcp/game/`.
`cargo run --release --bin th08 -- "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night"` (or TH08_DIR env).

## Next (RE and IMPL pipelines run in parallel, one Opus agent each; IMPL uses specs/IMPL_TEMPLATE.md)
1. RE phase3-opus (running): 08 bullets/items, 09 player, answers to docs/QUESTIONS.md.
2. IMPL impl4 (running): web parity, ?stage= params, tile seams.  Next IMPL: ECL VM + enemies (07_ecl ready).
3. IMPL: bullets/items/player/collision (after phase 3).
4. RE: 10 MSG + 11 RNG/stage flow/menus.  5. IMPL: dialogue, stage flow, HUD, menus.
6. RE/IMPL: replays, score.dat, config, ending/staff roll, sound effects.
