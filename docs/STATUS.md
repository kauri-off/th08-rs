# Status

## Done
- `src/archive.rs` — PBGZ th08.dat reader (header/table cipher, LZSS, edz payload wrapper). All 317 files byte-identical to DeepSeek's extraction. `cargo run --release --bin datdump <th08.dat> <out>`.
- `src/audio.rs` — thbgm.fmt parse + cpal stream, linear resampling to device rate, intro then loop to `+0x18` (loop start ASSUMED, unconfirmed). Device pick: pipewire → pulse → default (ALSA default is broken on this machine).
- `src/app.rs` — winit app, fixed 60 fps accumulator, input bitmask (`src/input.rs`), shows title00.png, plays th08_01.
- `src/gfx.rs` — wgpu sprite batcher; `capture()` for screenshots. `web/` — index.html + build.sh (wasm-bindgen --target web).
- Debug: `TH08_SHOT=<frame>` saves /tmp/th08_shot.png (offscreen 640x480 render) and exits. Use `RUST_LOG=warn,wgpu_hal=off` (a broken Vulkan layer spams errors).

- `src/anm.rs` — ANM chunks/sprites/scripts + THTX→RGBA (visually verified: player00 fmt5, eff01 fmt3). `cargo run --release --bin anmview <th08.dat> <x.anm> <outdir>` dumps textures with sprite rects.

## Run
Web: `web/build.sh`, serve `web/dist` (e.g. `python3 -m http.server -d web/dist 8087`). Verified in Playwright Chromium (WebGL2 fallback, OPFS cache works).
Playwright can only upload from the repo dir: game files are hard-linked into `.playwright-mcp/game/`.
`cargo run -- "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night"` (or TH08_DIR env).

## Next
1. Render ANM sprites in game (needs ANM opcode semantics → Phase 2 spec).
2. Phase 2 brief: ANM script VM, ECL VM, player, bullets, items, collision, STD backgrounds, MSG.
3. Engine: sprite batcher with blend modes (macroquad materials), playfield 384x448 at (32,16) — UNCONFIRMED.
