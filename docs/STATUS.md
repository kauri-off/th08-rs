# Status

## Done
- `src/archive.rs` — PBGZ th08.dat reader (header/table cipher, LZSS, edz payload wrapper). All 317 files byte-identical to DeepSeek's extraction. `cargo run --release --bin datdump <th08.dat> <out>`.
- `src/bgm.rs` — thbgm.fmt parse + cpal stream, intro then loop to `+0x18` (loop start ASSUMED, unconfirmed). Device pick: pipewire → pulse → default (ALSA default is broken on this machine).
- `src/main.rs` — 640x480 window, fixed 60 fps accumulator, input bitmask (`src/input.rs`), shows title00.png, plays th08_01.
- Debug: `TH08_SHOT=<frame>` saves /tmp/th08_shot.png and exits (Wayland, no external screenshot tool).

## Run
`cargo run -- "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night"` (or TH08_DIR env).

## Next
1. Phase 1 results → THTX decode (fmt 5 = ARGB4444?, fmt 3 = ?), ANM sprite rects, render ANM sprites.
2. Phase 2 brief: ANM script VM, ECL VM, player, bullets, items, collision, STD backgrounds, MSG.
3. Engine: sprite batcher with blend modes (macroquad materials), playfield 384x448 at (32,16) — UNCONFIRMED.
