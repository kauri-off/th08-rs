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

## Run
Web: `web/build.sh`, serve `web/dist` (e.g. `python3 -m http.server -d web/dist 8087`). Verified in Playwright Chromium (WebGL2 fallback, OPFS cache works).
Playwright can only upload from the repo dir: game files are hard-linked into `.playwright-mcp/game/`.
`cargo run --release --bin th08 -- "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night"` (or TH08_DIR env).

## Next (RE and IMPL pipelines run in parallel, one Opus agent each; IMPL uses specs/IMPL_TEMPLATE.md)
1. RE phase2-opus (running): 05 ANM VM, 06 STD, 07 ECL.
2. IMPL: ANM VM + sprite rendering — done (z-write/depth buffer and stage camera wait for STD).
3. RE: 08 bullets/items + 09 player.  4. IMPL: STD backgrounds + 3D camera/fog.
5. IMPL: ECL VM + enemies.  6. IMPL: bullets/items/player/collision.
7. RE: 10 MSG + 11 RNG/stage flow.  8. IMPL: dialogue, stage flow, HUD, menus.
9. RE/IMPL: replays, score.dat, config, ending/staff roll, sound effects.
