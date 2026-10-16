# Status

## Done
- `src/archive.rs` — PBGZ th08.dat reader (header/table cipher, LZSS, edz payload wrapper). All 317 files byte-identical to DeepSeek's extraction. `cargo run --release --bin datdump <th08.dat> <out>`.
- `src/audio.rs` — thbgm.fmt parse + cpal stream, linear resampling to device rate, intro then loop to `+0x18` (loop start/end VERIFIED by specs/opus/02_bgm.md). Device pick: pipewire → pulse → default (ALSA default is broken on this machine).
- `src/app.rs` — winit app, fixed 60 fps accumulator, input bitmask (`src/input.rs`), shows title00.png, plays th08_01.
- `src/gfx.rs` — wgpu sprite batcher; `capture()` for screenshots. `web/` — index.html + build.sh (wasm-bindgen --target web).
- Debug: `TH08_SHOT=<frame>` saves /tmp/th08_shot.png (offscreen 640x480 render) and exits. Use `RUST_LOG=warn,wgpu_hal=off` (a broken Vulkan layer spams errors).

- `src/anm.rs` — ANM chunks/sprites/scripts + THTX→RGBA (visually verified: player00 fmt5, eff01 fmt3). `cargo run --release --bin anmview <th08.dat> <x.anm> <outdir>` dumps textures with sprite rects.

## Run
Web: `web/build.sh`, serve `web/dist` (e.g. `python3 -m http.server -d web/dist 8087`). Verified in Playwright Chromium (WebGL2 fallback, OPFS cache works).
Playwright can only upload from the repo dir: game files are hard-linked into `.playwright-mcp/game/`.
`cargo run -- "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night"` (or TH08_DIR env).

## Next (one sub-agent at a time; RE = Opus dirty room, IMPL = Opus implementer via specs/IMPL_TEMPLATE.md)
1. RE phase2-opus (running): 05 ANM VM, 06 STD, 07 ECL.
2. IMPL: ANM VM + sprite rendering (blend modes, 2D/3D), title/menu via ANM; playfield viewport (32,16,384x448).
3. RE: 08 bullets/items + 09 player.  4. IMPL: STD backgrounds + 3D camera/fog.
5. IMPL: ECL VM + enemies.  6. IMPL: bullets/items/player/collision.
7. RE: 10 MSG + 11 RNG/stage flow.  8. IMPL: dialogue, stage flow, HUD, menus.
9. RE/IMPL: replays, score.dat, config, ending/staff roll, sound effects.
