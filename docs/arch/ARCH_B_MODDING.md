# Proposal B — content and mod platform

Goal: translation and asset replacement are first-class features of the port, and vanilla (no packs mounted) stays
bit-exact in logic and visually identical in drawing. This proposal builds on the baseline workspace
(`th08-formats` / `th08-core` / `th08-render` / `th08-audio` / `th08` app / `th08-tools`) and adds one crate,
`th08-content`. Measurements below are from the tree at commit e73440b (src/ ≈ 51k lines).

## 0. Core idea in five rules

1. **Two content classes, kept apart by crate boundaries.**
   *Presentation content* (texture pixels, display text, fonts, sound/music samples) can never reach game logic.
   *Logic content* (ECL, STD, SHT, MSG scripts, ANM sprite rects + scripts, `.end` command structure, replays) is
   everything the simulation reads. A pack declares which class it is. Cosmetic packs cannot change logic.
   We do not trust the pack to stay cosmetic; the type system enforces it, because the core only ever sees the
   *logic view* of the file system.
2. **The core never turns content into pixels, samples or display strings.** It outputs references: a texture key,
   a `TextRef` plus the original bytes, a sound id, a music track name. The presentation side resolves them through the
   *presentation view*, which includes cosmetic packs.
3. **Persisted and logic strings stay raw bytes.** Spell names go into score.dat with a checksum over their bytes
   (15 §2.5, `scoredat.rs:270`, `:717–732`), and replays carry Shift-JIS user text (`replay.rs:609`). The core keeps
   these as `[u8]`. A translation never changes a byte that is written to disk or read by logic.
4. **Vanilla is the identity case.** With no packs, every resolver falls through to the archive bytes and to today's
   code path (same decoder, same rasteriser, same mixer). The checks are the ones we already have:
   verify.zsh sessions, pilot, ref500/ref_b2, drawset images.
5. **Isolation test: the "chaos pack".** A test pack replaces *every* presentation key with garbage: random pixels,
   random strings of random length, silence, a different font. With it mounted, all logic traces must stay exact.
   This one test proves rule 1 and runs in CI.

## 1. Target architecture

### 1.1 Crates

```
th08-formats   pure parsers + writers: PBGZ read/repack (01), ANM split into AnmLogic + AnmPixels (03),
               ECL/STD/SHT/MSG/END/musiccmt/thbgm.fmt, PO/TOML string tables are NOT here (port-only formats)
th08-content   NEW: Vfs, Pack, Manifest, LogicView / PresentView, fingerprints, string tables (TextDb),
               pack sources (dat, dir, zip; web File/fetch). No wgpu/cpal. Builds for wasm.
th08-core      takes `LogicView` at construction; outputs FrameOut { draw: DrawList, text: Vec<TextCmd>,
               sound: Vec<SoundEv>, music: Vec<MusicEv> }. No `String` for game text, no GPU ids.
th08-render    TextureResolver (archive THTX / jpg / override PNG, HD scale), FontProvider + text layout,
               consumes DrawList + TextCmd
th08-audio     SampleResolver (se_*.wav / override), MusicResolver (thbgm.fmt + thbgm.dat / override ogg,flac / MIDI)
th08 (app)     mounts packs from settings, launcher pack list, hot-reload watcher (native)
th08-tools     extract, repack, pot (string template), pack-check, pack-new, thcrap-import (later)
```

### 1.2 Virtual file system

Names follow the original's lookup: the directory is stripped and names are compared case-insensitively
(01 §5, §7). So the VFS key is the lower-case basename. That is also the key packs use.

```rust
pub struct Vfs { layers: Vec<Layer> }            // last mounted wins
struct Layer { pack: PackId, class: Class, src: Box<dyn PackSource> }
pub enum Class { Vanilla, Logic, Presentation }
pub trait PackSource { fn read(&self, key: &str) -> Option<Bytes>; fn keys(&self) -> Vec<String>; }
//   impls: DatSource (PBGZ), DirSource (loose files), ZipSource (zip crate, pure Rust, wasm ok), WebSource

pub struct LogicView<'a>(&'a Vfs);    // Vanilla + Logic layers only
pub struct PresentView<'a>(&'a Vfs);  // every layer
impl LogicView<'_> { pub fn read(&self, name: &str) -> Result<Bytes>; pub fn fingerprint(&self) -> Fingerprint }
```

- `Vanilla` = `th08.dat` (+ `thbgm.dat` ranges for audio, 02 §1). The from-disk path of 01 §7 (score.dat, th08.cfg,
  replays) is **not** part of the VFS. It stays the save storage (`scoredat::storage_*`).
- `Fingerprint` = hash (blake3 or sha2) over the sorted (key, bytes) of all Logic-layer overrides.
  Vanilla = the empty fingerprint. Proposal C needs the same value for its netplay handshake.
- Pack layout (dir or zip):

```
pack.toml            id, name, version, class = "presentation" | "logic", language, priority, depends = [...]
files/               raw archive-name overrides (logic packs: *.ecl, *.std, *.anm …; presentation: *.jpg, *.png, se_*.wav)
images/<anm chunk image name>.png   per-texture pixel override (+ optional `scale = 2` in pack.toml [images])
text/<lang>.po       string table (msgctxt = TextRef key, msgid = original text, msgstr = translation)
fonts/*.ttf|otf      font files + [fonts] section: which TextTarget uses which face/size/offset
music/<track>.ogg    + [music.<track>] loop_start / loop_end in samples (default: thbgm.fmt values, 02 §2)
```

A presentation pack that puts a logic key (`.ecl`, `.std`, `.sht`, msg `.dat`, `.anm`, `.end`, `.rpy`, `.ver`,
`.fmt`) under `files/` is rejected at mount time (and by `pack-check`). `.anm` is a logic key because sprite rects
drive logic (§2). Its pixels are replaced through `images/` instead.

### 1.3 Data flow per frame

```
input ──► core.step(&LogicView-loaded data, input) ──► FrameOut
                                                        ├─ DrawList { TexKey{anm, chunk} | ImageKey(name), quads… }
                                                        ├─ TextCmd  { target, TextRef, raw: [u8;≤128], colour, outline, align }
                                                        ├─ SoundEv(id) / MusicEv{Load(slot,name) | Play | Fade(s) | Stop}
render: TextureResolver(PresentView) + TextLayout(TextDb, FontProvider) ──► GPU
audio : SampleResolver / MusicResolver(PresentView) ──► cpal
```

`TextCmd` replaces today's in-calc rasterisation. Text_RenderToVm (10 §6) becomes "write this text into this strip
of this `@` render target" and the render side runs it at the same point in the command order as today, so the
strip contents and their lifetime rules (10 §6: whole 512-texel row cleared, rows Y+15/16 untouched) stay as they are.

### 1.4 Text references

```rust
pub enum TextRef {
    Msg { file: u8, entry: u16, nth: u16 },         // nth text op (3/16/19/20) in the entry, 10 §5.1
    Spell { card: u16, field: SpellField },          // Name | Owner | Comment0 | Comment1; card id ECL +0x0E (07 §11.1)
    PlayerSpell { chara: u8, last: bool },           // exe table, 14 §2 (bomb.rs SPECS)
    Ending { file: u8, line: u16 },                  // nth text line of the .end file, 16 §1.3
    MusicCmt { track: u8, line: u8 },                // musiccmt.txt, 16 §5.1
    Builtin { id: BuiltinStr, args: FmtArgs },       // strings that are exe literals today (menus, result, replay UI)
    Dynamic,                                         // player names, digits: raw bytes only, never translated
}
```

- The key form in PO is stable and readable: `msg/msg4a.dat/3/17`, `spell/112/name`, `end/end00.end/5`,
  `builtin/result.card_name`. `pot` generates msgid = decoded vanilla text, plus extracted comments
  (speaker, stage, entry) as translator context.
- Lookup: `TextDb::get(&TextRef) -> Option<&str>`. On a miss the presentation side decodes `raw` with the vanilla
  Shift-JIS decoder (`text.rs:5`). That is exactly today's path, so vanilla output is byte-identical.
- Score data, the spell-practice list (`result.rs:574` reads names from CATK) and the replay info text keep the
  raw bytes. They are *displayed* through `TextRef::Spell { card, .. }`, so a translated spell name shows up even
  though score.dat stores the Japanese bytes.

### 1.5 Layout policy for translated text

Logic fixes the number of lines and their timing: msg line waits come from op 4 (10 §5.2), and every ending text line
costs one VM and one line wait (16 §1.3). A translation may therefore never add or drop lines. Per `TextTarget`
(msg line strip 320×17, ending strip, spell plate, music comment …) the manifest picks
`overflow = "squeeze" | "shrink" | "clip"` (horizontal scale / smaller font size / cut). An empty msgstr means
"keep vanilla". Right-aligned text (Text_RenderToVmRight, `gui.rs:215`: x = u0 + sw − len·15/2 with len in bytes)
uses the byte formula on the vanilla path and the measured pixel width for translated text. This only moves pixels
inside a strip and does not affect logic.

### 1.6 Fonts

`text.rs` is a SPEC-GAP today: GDI "ＭＳ ゴシック" is not reproducible. The port uses a system font on native
(`FONT_PATHS`, ab_glyph) and the browser canvas on web, so native and web pixels differ.

```rust
pub trait FontProvider { fn mask(&mut self, s: &str, face: FaceId, cell: f32, x: f32, y: f32, w: usize, h: usize) -> Vec<u8>; }
// SystemFont (today's native path), CanvasFont (today's web path), FileFont (ab_glyph on a pack TTF — native and wasm)
```

The 2× work surface, the 4-offset black outline and the D3DX TRIANGLE 2:1 downsample (10 §6) stay shared code
around `mask()`. With no pack, `SystemFont`/`CanvasFont` behave exactly as now. A pack font goes through `FileFont`
on every platform, so native and web give the same pixels, which is a side benefit. Translations need fonts that
cover their script: Latin-only, Cyrillic, Hangul and Hanzi go in the font pack, and `pack-check` reports characters
in the PO that the font lacks.

### 1.7 Images

- `TexKey { anm: AssetKey, chunk: u16 }` → THTX pixels from the archive, or `images/<chunk image name>.png`
  (chunk name at +0x1C, 03 §2.1). Sprite UVs are normalised by the chunk's *logical* size, not by the THTX size
  (03 §2.2, `anmmgr.rs:139`). So an override may be any integer multiple of the logical size (HD). Rects, scripts and
  logical size always come from the logic view.
- Standalone `.jpg`/`.png` (03 §1; `th08logo.jpg`, `result.jpg`, ending pictures `@b`) are overridden by name.
- `@` render-target chunks (text strips, capture surfaces) cannot be overridden.
- HD only pays off with an internal render scale > 1. The vanilla scale-1 renderer stays the reference.
  Scale is a render option, so it does not touch the parity policy.

### 1.8 Audio

- SE: core emits the SE id; the 13 §1.1 table (wav index, volume, priority) stays in core/audio as today. The sample
  comes from `se_*.wav` in the presentation view (any `hound`-readable wav, or ogg via lewton/symphonia).
- BGM: `MusicEv::Load(slot, name)` carries the STD/msg/`@m` name. Bgm_FindTrack (02 §4, `audio.rs:41–49`, including
  the kept typo fallback to track 0) runs unchanged on vanilla names first. Then the resolver checks `music/<track>`
  overrides with loop points, else the thbgm.dat range. The MIDI path (02 §5) gets a soundfont per pack.
- Audio never feeds back into logic. Measured: `ctx.audio` is used in 9 modules, all calls are outbound
  (stop/pause/fade/play/load), and the only read is `is_started()` in `game.rs:141` (device start, not game state).

### 1.9 Logic (gameplay) packs: designed in, built last

Logic packs put ECL/STD/SHT/MSG/ANM/END overrides into the logic view. Everything above still applies. What changes:
- The fingerprint is non-empty. Replays, score.dat and the cfg are saved in a namespace per fingerprint
  (`<save_dir>/packs/<id>-<fp8>/`), because the original replay format has no field for it and must stay readable by
  th08.exe. The session/trace tools record the fingerprint in their metadata.
- Vanilla replays are refused, or played with a warning, while a logic pack is mounted.
- `repack` can build a th08.dat for the original exe from vanilla + a logic pack, so a mod can be checked against
  th08.exe under wine with the existing RE tooling.

### 1.10 Hot reload (optional, native)

A `notify` watcher on mounted dir packs sends `ContentChanged(keys)`:
- presentation keys: drop the cached texture / sample / TextDb entry and rebuild it on next use. This is safe
  mid-game because logic never saw it;
- logic keys: apply at the next stage restart only (the snapshot/replay stays consistent). Hot reload is off during
  replay playback, session recording and netplay.

## 2. Mapping from today's src/ (measured)

| Today | Finding | Goes to |
|---|---|---|
| `archive.rs` (418 l.) | One reader, `Archive::read(name)` (lower-case basename, 01 §5–§7); writers lzss_compress / block_encrypt already exist (used by score.dat / replay) | PBGZ read + repack → formats; `Archive` becomes `DatSource` |
| `game.rs:19,62` `Ctx.archive` | **Single content chokepoint**: about 45 call sites in ~20 modules use `ctx.archive.read` or `anm.load(&mut gfx, &ctx.archive, slot, name)` | `Ctx.archive` → `LogicView`; call sites keep their shape |
| `assets.rs` (104 l.) | `Source` native dir / web File list, range reads for thbgm.dat | `PackSource` impls in content |
| `anmmgr.rs:95–110` | `load` parses **and** uploads THTX to the GPU in one call | split: core keeps `AnmLogic` (rects, scripts, logical size); render gets `TexKey` |
| `enemy.rs:1301`, `bullet.rs:354,1138`, `pshot.rs:775`, `bomb.rs:1538` | `vm.sprite_w/h` (from ANM sprite records) drive culling, collision boxes, laser scale | proves `.anm` is logic content; images override pixels only |
| `text.rs` (353 l.) | Rasteriser + font search; reads `settings::get().font` (global) | `FontProvider` + outline/downsample → render |
| 7 modules own a `text::Raster`: ending, gui, hud, menu, menulist, musicroom, result | 17 raster call sites | each emits `TextCmd` instead |
| `hud.rs:467,1276–1278` | Dialogue lines rasterised and written to the GPU texture **inside HUD calc** | calc/draw mixing to untangle first (needed by baseline too) |
| `gui.rs:68,245,408` | Spell-name plate queue `(VM, text, colour, x)` already deferred | queue element → `TextCmd` (already almost the target shape) |
| `msg.rs:148` `string()` | XOR-0x77 SJIS decoded to `String` in the interpreter | keep raw bytes + `TextRef::Msg` |
| `eclvm.rs:1464–1499` | Spell name/owner/comments decoded from ECL as bytes, copied to Gui + CATK | `TextRef::Spell{card}` for display; bytes unchanged for CATK |
| `eclvm.rs:1265` | native 22 encodes literal 「リザレクション」 | `Builtin` |
| `ending.rs:445` | text lines decoded, rendered in calc | `TextRef::Ending{file,line}` |
| `musicroom.rs:62` | musiccmt.txt parsed into byte lines | `TextRef::MusicCmt` |
| Japanese literals in src | menulist 67, menu 32, result 32, replay 20, bomb 17, musicroom 9 | `BuiltinStr` table (one generated module); bytes that go to files (replay user text, score.txt 16 §2.7) stay vanilla |
| `ctx.gfx` | 144 uses in 18 modules | core must lose all of these (baseline goal), then textures are keyed, not ids |
| `audio.rs` | outbound only (see §1.8) | events; resolvers in th08-audio |
| statics | ~35 `static`/`thread_local` in 12 modules (gamemgr 6, ascii 6, hud 4, effect 4, …); 20 `settings::get()` | not blocking for B except `text.rs` font setting and `gui::request` queue (global) → passed in |

Archive contents (01 §9, 317 entries): 113 anm, 39 wav, 37 jpg, 33 dat (msg), 24 ecl, 22 mid, 18 std, 14 end,
8 sht, 4 rpy, 2 png, 1 txt (musiccmt), 1 ver, 1 fmt. Translation volume: about 2 240 msg text ops (10 §5.1:
op 16 ×2214, op 3 ×18, ops 19/20 ×8), 222 spell cards × 4 fields (15 §2.5), 14 ending files, ~180 builtin literals.

## 3. Migration plan (each step verifiable)

Steps M1–M3 fit into the baseline split order (formats → core). M4 and later come after render/audio are split.

| # | Step | Check |
|---|---|---|
| M0 | `th08-content` with `Vfs` + `DatSource`; unit test: for all 317 names `vfs.read == Archive::read` | cargo test (needs local th08.dat, like `test_archive()`) |
| M1 | `Ctx.archive: Archive` → `Ctx.fs: Vfs` with the same `read` API; web `Source` → `WebSource` | verify.zsh + pilot + ref500/ref_b2 exact; wasm build |
| M2 | ANM split: `AnmLogic` in core, `TexKey` in draw items, render-side texture cache from `AnmPixels` | traces exact; drawset images identical |
| M3a | Dialogue: `Dialogue.texts` element `(line, String, colour)` → `TextCmd{TextRef::Msg, raw}`; rasterise on render side at the same command point | traces exact; golden draw images identical (same rasteriser) |
| M3b | Same for gui (spell plates), ending, musicroom, menulist/menu/result; `BuiltinStr` table replaces literals | per module: traces exact + images |
| M3c | Core holds no `String` for game text (grep gate: no `decode_sjis` in th08-core except debug logs) | grep in CI |
| M4 | Pack mount (dir + zip), manifests, PresentView; image overrides | chaos pack: all sessions/batches exact; identity pack (re-encoded vanilla PNGs) → images identical |
| M5 | `TextDb` (PO), `th08-tools pot`; pseudo-locale pack (accents, +40 % length) | chaos pack exact; manual look at overflow modes; `pack-check` on the pseudo pack |
| M6 | `FontProvider`; `FileFont` on native + wasm | vanilla images identical; font pack gives same pixels native vs wasm (image diff) |
| M7 | Audio overrides (SE, music with loop points, soundfont) | traces exact; PCM diff of identity music pack vs vanilla = 0 |
| M8 | Tools: extract, repack (round trip `repack(extract(dat))` parses identically), pack-new, pack-check | round-trip test |
| M9 | Launcher pack list (settings table, `project-launcher`), web: pick zips with the dat files, `?pack=url` | manual; wasm build |
| M10 | Hot reload (native) | manual; off for session/replay runs (assert) |
| M11 | Logic packs: logic layers, fingerprint, save namespace, session metadata | vanilla fingerprint = empty → all checks exact; demo logic pack (one ECL tweak) records + replays its own session exactly |

Effort, rough: M0–M3 ≈ 2 IMPL cycles (much of it is work the baseline needs anyway: gfx-free core). M4–M7 ≈ 3 cycles
(≈ 2.5k new lines). M8–M11 ≈ 2–3 cycles. A thcrap importer (thpatch has many existing th08 translations) is
1 extra cycle. Its input formats are port tooling, not game behaviour, and must be checked against real thcrap
files before it is written.

## 4. Requirements on the core

### Decide at the start of the rewrite (expensive to retrofit)

- **D1 — content access only through `LogicView`.** The core gets content as bytes by archive name from a handle
  passed in at construction. No `Archive`, no paths, no `Source` in the core. Retrofitting means touching all ~45
  call sites again.
- **D2 — no GPU or audio handles in core state.** Draw items name `TexKey`/`ImageKey`, not `TextureId`. ANM is parsed
  into logic + pixels. (The baseline wants a wgpu-free core too; B adds that the *key* type must be stable and content
  addressed, so the render side can swap pixels.)
- **D3 — text leaves the core as `TextCmd { TextRef, raw }`.** Text_RenderToVm (10 §6) is an output command. It is
  never done inside calc. The `TextRef` variants must be designed when msg/ecl/ending/gui/menus are rewritten,
  because adding them later reopens every text module.
- **D4 — game text in the core is `[u8]` (Shift-JIS), never `String`.** Idiomatic-Rust pressure (proposal A) will push
  towards `String`; that loses bit-exact persistence (CATK checksum, replay user text) and the identity fallback.
- **D5 — the logic fingerprint exists from day one** (empty for vanilla) and goes into session/trace metadata. C needs it too.
- **D6 — sound and music are events by id/name** (`SoundEv`, `MusicEv`); Bgm_FindTrack runs on names (02 §4) before
  any override lookup.

### Can be added later (cheap if D1–D6 hold)

PO/TOML string tables, pseudo-locale, fonts per pack, HD textures and render scale, music codecs, zip/web pack
sources, hot reload, the launcher UI, logic packs and save namespaces, repack, thcrap import.

## 5. Compatibility, risks

### With A (idiomatic core)

- Shared: no globals, typed ids, ownership of `Ctx` parts, crate split. D1/D2 are the same thing A wants for
  testability. `TexKey`, `TextRef`, `SoundId` are the typed ids A asks for.
- Conflict 1: A may want `String`/`&str` for text in the core. B needs raw bytes (D4). Resolve: a `SjisStr` newtype
  (`Box<[u8]>` with `Display` via the decoder) gives A its type safety and keeps B's bytes.
- Conflict 2: A's ANM design must keep parse (logic) and upload (pixels) separate (D2); one `AnmManager` owning
  textures does not fit.
- A's error handling helps: missing/invalid pack files must be reported errors, never panics, while vanilla keeps the
  original's behaviour (missing archive member → start-up fails, 01 §7 / 15 §2.10).

### With C (netplay-ready core)

- Shared: gfx-free core state (D2) keeps snapshots small (no textures, no strings). D5's fingerprint is C's content
  handshake: peers must have equal *logic* fingerprints.
- Gain: cosmetic packs are per peer. One player can play in English and the other in Japanese with an HD pack, and
  rollback is unaffected, because presentation never enters state or hashes. The chaos-pack test is the same kind of
  proof C needs.
- Rule: hot reload of logic keys is disabled in netplay and replays (§1.10). TextDb/texture caches live outside the
  snapshot. `TextCmd`s re-emitted during rollback re-simulation are idempotent (same strip, same text), so render
  can apply only the confirmed frame's commands, like the draw list.
- C's 2-player modes would be logic content. If they are shipped as data, they are logic packs with a fingerprint.

### Risks

- **Hidden logic reads of presentation data.** Sprite rects are already known (§2). If any other presentation value
  (text width, image size, sample length) turns out to feed logic, the chaos pack finds it. The fix is then to move
  that value into the logic view, not to weaken isolation.
- **Text strips are shared render targets.** Writing the whole 512-texel row (10 §6) is a quirk that translated
  layout must respect: a long line must not bleed into the neighbouring strip. The render side clips to the strip.
- **Key stability.** `TextRef` keys assume the one retail data version (v1.00d, `th08_0100d.ver`). The `pot` output
  stores msgid, so a translation can be re-matched by original text if a key ever shifts.
- **Font coverage and licensing.** Packs ship their own fonts (OFL fonts are fine). The port ships no font, and the
  vanilla font path stays a SPEC-GAP.
- **Legal.** Packs must contain only new content. `extract`/`repack` work on the user's own th08.dat. Pack-check warns
  when a `files/` override is byte-identical to a vanilla member (redistributing original data).
- **wasm memory.** HD packs in a browser: zip entries are read lazily and the texture cache is bounded.
- **Scope creep.** M4–M11 are optional for "port complete". Only D1–D6 (cost: small, mostly overlapping with the
  baseline) are needed before the core is rewritten.
