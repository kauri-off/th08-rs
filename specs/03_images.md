# 03 — Images, textures and the ANM container

Tools: `tools/anm.py <file|dir> [--png outdir] [--scripts]` parses every `.anm` in the archive,
validates headers/THTX sizes, converts embedded textures to PNG and dumps raw script instructions.
Run over all 113 ANM files (310 texture entries): **0 problems** (VERIFIED). PNG conversions of
`etama.anm` (A4R4G4B4), `front.anm` (A8R8G8B8), `eff01.anm` (R5G6B5) were inspected visually and are
correct (VERIFIED).

## 1. Standalone image files in th08.dat

| type | count | notes |
|---|---|---|
| `.jpg` | 37 | baseline JFIF (VERIFIED signature): ending pictures `end0*.jpg`, staff roll `staff00*.jpg`, `info00-02.jpg`, `phantasm.jpg`, `music.jpg` (music room), `result.jpg`, `th08logo.jpg` (title logo, loaded as `title/th08logo.jpg`). Loaded through the D3DX image loader. |
| `.png` | 2 | `title00.png` (loaded as `title/title00.png` by the title screen), `select00.png`. Standard PNG. Loaded through D3DX. |

The engine refers to them with directory prefixes (`title/…`, `result/…`) which the archive lookup
strips (01_archive.md §5).

## 2. ANM file = chain of texture entries

An `.anm` file is a sequence of **entries**, each describing one texture, its sprites and its
animation scripts. Entry *k+1* starts at `entry_k_start + entry_k.next_offset`; a `next_offset`
of 0 ends the chain. All offsets inside an entry are **relative to that entry's start**.
(Loader: `Anm_Preload` 0x465860, `Anm_LoadEntryTexture` 0x465CC0.) All integers little-endian.

### 2.1 Entry header (0x40 bytes)

| off | size | type | name | meaning | conf. |
|---|---|---|---|---|---|
| 0x00 | 4 | u32 | sprite_count | number of sprite offsets that follow the header | VERIFIED |
| 0x04 | 4 | u32 | script_count | number of script table entries after the sprite offsets | VERIFIED |
| 0x08 | 4 | u32 | — | always 0 in th08 data; never read by the loader | VERIFIED RE-Q26 |
| 0x0C | 4 | u32 | width | logical texture width; sprite UVs are normalised by this | VERIFIED |
| 0x10 | 4 | u32 | height | logical texture height | VERIFIED |
| 0x14 | 4 | u32 | format | texture format index (§3) | VERIFIED |
| 0x18 | 4 | u32 | color_key | ARGB colour key, only passed to the external-image loader `Anm_CreateTextureFromImage`; 0xFF000000 in 3 entries, 0 elsewhere — all of them have a THTX block, so the key is never used with retail data | VERIFIED RE-Q26 |
| 0x1C | 4 | u32 | name_offset | offset of NUL-terminated image name (e.g. `data/etama/etama.png`); a name starting with `@` means "create a blank texture of width×height×format" (render target for text / screen capture) | VERIFIED |
| 0x20 | 4 | u32 | x | always 0 in data; never read | VERIFIED RE-Q26 |
| 0x24 | 4 | u32 | y | always 0 in data; never read | VERIFIED RE-Q26 |
| 0x28 | 4 | u32 | version | must be **3**, otherwise load fails | VERIFIED |
| 0x2C | 4 | u32 | priority | passed to the texture's resource priority (D3D `SetPriority`); values 0/1/10/11 | VERIFIED call, meaning INFERRED |
| 0x30 | 4 | u32 | thtx_offset | offset of embedded THTX block | VERIFIED |
| 0x34 | 1(+3) | u8 | has_thtx | non-zero → texture comes from the THTX block; zero → load image named at name_offset from **disk** (unless name starts with `@`) | VERIFIED |
| 0x38 | 4 | u32 | next_offset | offset to the next entry, 0 = last | VERIFIED |
| 0x3C | 4 | u32 | — | 0 in all 310 entries; never read | VERIFIED RE-Q26 |
| 0x40 | 4×N | u32[sprite_count] | sprite_offsets | offsets of sprite records | VERIFIED |
| … | 8×M | {i32 id, u32 offset}[script_count] | script table | the loader **ignores `id`** and uses only `offset`; scripts are addressed by cumulative index (see 2.4) | VERIFIED |

In th08.dat, every entry has a THTX block except 4 entries named `@` (capture.anm#0, music00.anm#1,
resulttext.anm#0, text.anm#0).

### 2.2 Sprite record (20 bytes, at entry_start + sprite_offset)

| off | type | meaning |
|---|---|---|
| 0x00 | u32 | sprite id — always equals the cumulative sprite index within the file (VERIFIED, all files); not used by the loader |
| 0x04 | f32 | x (left) in logical texture pixels |
| 0x08 | f32 | y (top) |
| 0x0C | f32 | width |
| 0x10 | f32 | height |

UVs: `u0 = x / header.width`, `v0 = y / header.height`, `u1 = (x+w) / header.width`,
`v1 = (y+h) / header.height` (VERIFIED from the loader arithmetic: it scales by
`actual_texture_width / header.width` and then divides by the actual texture width). Six entries have a
THTX larger than the header size (e.g. eff09sk.anm#1: header 256×256, THTX 2048×2048); the
normalised UVs make sprites cover the same fraction of the texture regardless.

The sprite's display size is (w, h) in logical pixels (runtime record stores width at +0x34,
height at +0x30).

### 2.3 Script instruction encoding

Scripts are byte streams of variable-length instructions:

| off | type | meaning |
|---|---|---|
| 0x00 | u16 | opcode (0xFFFF = end-of-script marker) |
| 0x02 | u16 | total instruction size in bytes (including this 8-byte header) |
| 0x04 | i16 | time (frame) at which the instruction executes; the VM executes all instructions whose time ≤ current script time |
| 0x06 | u16 | parameter mask: bit *n* set → argument *n* is a variable reference instead of an immediate |
| 0x08 | … | arguments, 4 bytes each (int or float) |

Interpreter: `AnmVm_Run` 0x45EA00, opcodes −1…0x58 via a jump table. Examples (VERIFIED encodings;
full semantics in 05_anm_vm.md):
opcode 3 `(size 12)` = set sprite (arg = cumulative sprite index; mask bit 0 → arg read from a variable),
opcode 2 `(size 8)` = stop, opcode 25 `(size 12)` = flag-setting op seen right after set-sprite.

### 2.4 Indexing across entries

When a file is loaded into one of **25 ANM slots** (0…24; index ≥ 25 is an error), its sprites from
all entries are concatenated into one array (cumulative index), and its scripts likewise
(VERIFIED in `Anm_Preload`/`Anm_LoadEntryTexture`). Scripts reference sprites by this
per-file cumulative index.

## 3. Texture formats

Format index → Direct3D 8 surface format and bytes per pixel (tables at 0x4B8480 / 0x4B8498, VERIFIED):

| index | D3DFORMAT (value) | bytes/px | pixel layout (little-endian word) | used by entries |
|---|---|---|---|---|
| 0 | unknown (0) — take from file | 4 | — | none |
| 1 | A8R8G8B8 (21) | 4 | bytes B,G,R,A | 170 |
| 2 | A1R5G5B5 (25) | 2 | `A:1 R:5 G:5 B:5` (bit 15 = alpha) | none |
| 3 | R5G6B5 (23) | 2 | `R:5 G:6 B:5` | 33 |
| 4 | R8G8B8 (20) | 3 | bytes B,G,R | none |
| 5 | A4R4G4B4 (26) | 2 | `A:4 R:4 G:4 B:4` (bits 15..12 = A) | 107 |

Reduced-colour option (`FUN_00443A90` returns true, a config flag — INFERRED "16-bit textures"):
the *creation* format is remapped: index 1 or 0 → 5 (A4R4G4B4), index 4 → 3 (R5G6B5); the source pixel
data is converted by D3DX on upload. A reimplementation can always upload at full precision.

## 4. THTX block (embedded texture)

| off | size | type | meaning |
|---|---|---|---|
| 0x00 | 4 | char[4] | `"THTX"` |
| 0x04 | 2 | u16 | 0 |
| 0x06 | 2 | u16 | format index (§3); equals the entry header format in all data |
| 0x08 | 2 | u16 | width (px) |
| 0x0A | 2 | u16 | height (px) |
| 0x0C | 4 | u32 | data size = width × height × bytes/px (VERIFIED for all 306 blocks) |
| 0x10 | … | | pixel rows, top to bottom, tightly packed (`width × bpp` bytes per row) |

Upload (VERIFIED from `Anm_CreateTextureFromThtx` 0x4655E0): an offscreen surface of the THTX
size/format is filled row by row, then a texture of the same size is created and the surface copied
into it with D3DX (filter "none", no colour key).

## 5. Runtime structures (for orientation)

Per loaded file (ANM manager + 0x24 + slot × 0x1C):

| off | meaning |
|---|---|
| 0x00 | slot index |
| 0x04 | pointer to file data (kept for scripts) |
| 0x08 | number of entries |
| 0x0C | sprite array (0x44 bytes each) |
| 0x10 | script pointer array (cumulative) |
| 0x14 | per-entry texture array (12 bytes each: texture, external image data, size) |
| 0x18 | incremental-load state (entries are loaded one per call while non-zero) |

See `structs.md` for the runtime sprite record.
