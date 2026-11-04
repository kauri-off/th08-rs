# ASSETS — a smaller format for the shipped game data (research report)

Brief: docs/briefs/ASSETS_BRIEF.md (2026-11-03). Question: the personal APK is ~511 MB (th08.dat 46.8 MB + thbgm.dat
450.0 MB of raw PCM + code). Can it be cut several times without touching logic?

**Answer: yes, by about 6×, losslessly for everything except the BGM.** Keep every th08.dat entry byte-identical and
only recompress it (zstd: 46.8 → 21.9–26.2 MB). Transcode thbgm.dat to Ogg Vorbis q4 (450.0 → 44.1 MB). Vorbis
decodes at 44.1 kHz, so the loops stay sample-exact by construction. Textures need no new format: the native THTX
data under zstd is smaller than PNG or lossless WebP. The arm64 APK goes from **512 MB to ≈ 81 MB**, or ≈ 75 MB with
Vorbis q3. Lossy textures and GPU formats save little disk space or none. They are an optional later VRAM project,
not part of this one.

All numbers below were measured on this machine (32 threads, Linux) with throwaway tools in
`target/assets_exp/tool` (bins `inv`, `bgmcheck`, `texexp`, `texbench`) and the scripts in
`target/assets_exp/scripts`. Results are in `target/assets_exp/results/`. All derived data is personal only and
lives under `target/assets_exp/`.

## 1. Inventory

### 1.1 th08.dat (01 §2–§9): 317 entries, 112.5 MB unpacked, 46.8 MB as LZSS archive

| kind | files | unpacked bytes | read by | code |
|---|---|---|---|---|
| `.anm` | 113 | 102 737 072 | **logic**: entry chain, sprites, scripts, header size (03 §2.1–§2.4). **Present**: THTX pixels (03 §3–§4) | `anmmgr.rs:143` `anm::parse(archive.read)`; `anmmgr.rs:145` `anm::textures(archive.present().read)` |
| `.ecl` | 24 | 1 011 064 | logic (07) | `enemy.rs:815` |
| `.std` | 18 | 75 372 | logic (06) | `stage.rs:431` |
| `msg*.dat` | 33 | 115 844 | logic (10) | `hud.rs:385` |
| `.sht` | 8 | 13 760 | logic (09) | `player.rs:623–624` |
| `.end` | 14 | 31 907 | logic (16) | `ending.rs:201` |
| `.rpy` (demos) | 4 | 12 547 | logic (15) | `supervisor.rs:589`, `menu.rs:617`, `worldclone.rs:713` |
| `thbgm.fmt` | 1 | 1 109 | logic (02 §2: track table, loop points) | `game.rs:289` `Bgm::new` |
| `.ver` | 1 | 356 | logic (15 §2.10) | `game.rs:286` |
| `musiccmt.txt` | 1 | 7 427 | logic (16 §5) | `musicroom.rs:320` |
| `.jpg` | 37 | 4 343 794 | present (03 §1), 640×480 (2 of them 640×720) | `menu.rs:188/429`, `result.rs:1324`, `musicroom.rs:310`, `ending.rs:322`, `supervisor.rs:301/354` → `rec.load_image` |
| `.png` | 2 | 999 748 | present (select00, title00) | same path |
| `se_*.wav` | 39 | 2 155 488 | present (13 §1.1); mixed 8/16-bit, mono/stereo, 22.05/44.1 kHz, several with trailing RIFF chunks | `audio.rs:542` `load_se` |
| `.mid` | 22 | 1 010 588 | present (02 §5, MIDI mode) | `audio.rs:372` `load_midi` |

THTX textures (`inv`, `results/inventory.tsv`): 310 ANM entries, of which 306 have a THTX block and 4 are `@` render
targets (capture.anm, music00.anm, resulttext.anm, text.anm). THTX by format (03 §3):

| THTX format | entries | THTX bytes | pixels |
|---|---|---|---|
| 1 A8R8G8B8 | 170 | 55 836 672 | 13 959 168 |
| 3 R5G6B5 | 32 | 10 485 760 | 5 242 880 |
| 5 A4R4G4B4 | 104 | 36 077 568 | 18 038 784 |
| total | 306 | 102 400 000 (99.7 % of all .anm bytes) | 37 240 832 → 149.0 MB as RGBA8 |

Sizes: 109× 256², 92× 128×256, 51× 512², 29× 256×512, 10× 256×64, 10× 512×256, 3× 128², 2× 2048² (eff09al/eff09sk,
16.2 MB RGBA8 each). Every size is a multiple of 4. Six entries have a THTX size that differs from the header's
logical size (eff05/06/07 #0, eff08 #3: header 256², THTX 512²; eff09al/sk #1: header 256², THTX 2048²). Sprite UVs
are normalised by the header size (03 §2.2, `anmmgr.rs:183`), so this is only a presentation detail. There are 23
byte-identical duplicate THTX blocks in 21 groups (eff05/06/07, eff09al/sk, face_*sp).

**What logic reads.** Everything except THTX pixels, jpg/png, wav and mid. Three presentation facts still reach core
code paths, and a repack must keep them:
- **THTX width/height** go into `rec.create_anm_texture(w, h)` and `set_label(…, c.width, c.height)`. They are read
  back through `Recorder::texture()` by the text / capture / copy helpers: `hud.rs:482/1561`, `gui.rs:274`,
  `menu.rs:452`, `menulist.rs:515`, `musicroom.rs:123`, `ending.rs:434`, `ascii.rs:1258`. All of these only build
  render commands, but the draw log prints the sizes. So **texture dimensions must stay unchanged** (true for every
  candidate below).
- **jpg/png size**: `drawlist.rs:696` decodes the image to learn its size (it moves to a header probe in 11p). Keeping
  the jpg/png bytes unchanged keeps it identical.
- **The thbgm.dat header probe** (`game.rs:296`, `config.rs:9`, 02 §1 / 15 §1.2) is logic. It decides start-up
  failure and music mode 2 when the defaults are applied. The repacked BGM must answer the 16-byte probe exactly as
  the original file does.

### 1.2 thbgm.dat (02 §1–§3): 21 tracks, 112 490 252 stereo frames = 42.5 min, 44.1 kHz s16

All values are frames. `loop_end` = frames stored in thbgm.dat. `+0x14` is larger and never read (02 §2).

| track | offset (B) | loop_start | loop_end | loop body | +0x14 | Vorbis q4 B | FLAC -8 B |
|---|---|---|---|---|---|---|---|
| th08_01 | 16 | 247472 | 3178816 | 2931344 | 3635870 | 1165475 | 7726686 |
| th08_00 | 12715280 | 174976 | 5782208 | 5607232 | 13450211 | 2182907 | 17821276 |
| th08_03 | 35844112 | 696320 | 3440640 | 2744320 | 3862167 | 1322309 | 10680551 |
| th08_04 | 49606672 | 121792 | 6599680 | 6477888 | 6797581 | 2707980 | 20115022 |
| th08_05 | 76005392 | 1127840 | 4244544 | 3116704 | 4740073 | 1675470 | 12614967 |
| th08_06 | 92983568 | 685184 | 4071424 | 3386240 | 5150879 | 1585748 | 10971091 |
| th08_07 | 109269264 | 1190272 | 5048960 | 3858688 | 5496275 | 1945060 | 13818753 |
| th08_08 | 129465104 | 365248 | 4202496 | 3837248 | 4812412 | 1642482 | 13063580 |
| th08_09 | 146275088 | 584384 | 7233536 | 6649152 | 8076701 | 2905432 | 21514866 |
| th08_10 | 175209232 | 453376 | 5588992 | 5135616 | 5699076 | 2181183 | 17297815 |
| th08_11 | 197565200 | 294592 | 7694912 | 7400320 | 7949658 | 3110499 | 23658115 |
| th08_12 | 228344848 | 133536 | 5300096 | 5166560 | 5635792 | 2055620 | 16636179 |
| th08_13 | 249545232 | 416432 | 3812648 | 3396216 | 3984757 | 1465886 | 10554410 |
| th08_14 | 264795824 | 308928 | 7856384 | 7547456 | 8146170 | 3121402 | 24870104 |
| th08_13b | 296221360 | 3090656 | 3612416 | 521760 | 3985333 | 1445382 | 11154060 |
| th08_15 | 310671024 | 1669568 | 7937024 | 6267456 | 8772515 | 3204367 | 25091615 |
| th08_16 | 342419120 | 771712 | 3956864 | 3185152 | 4144619 | 1408995 | 9639506 |
| th08_17 | 358246576 | 858720 | 4650304 | 3791584 | 4817801 | 1834340 | 11839475 |
| th08_18 | 376847792 | 220320 | 7003936 | 6783616 | 7666039 | 2708430 | 20623229 |
| th08_19 | 404863536 | 333704 | 7796196 | 7462492 | 8064732 | 3109896 | 23771906 |
| th08_20 | 436048320 | 276224 | 3478176 | 3201952 | 3526950 | 1331330 | 9493846 |

Playback today (`audio.rs:520`) reads `loop_end·4` bytes by range. It converts them to `Vec<[f32; 2]>` (largest track
th08_15: 63.5 MB, plus a 31.7 MB transient read buffer). `Music::track_frame` plays `[0, le)` and then
`[ls, le)` forever (02 §3).

## 2. Experiments

### 2.1 BGM

Encoders: `flac -8`, ffmpeg libvorbis `-q:a N`, ffmpeg libopus VBR `compression_level 10` (resampled to 48 kHz by
ffmpeg). Decoders: the pure-Rust crates the port would use: claxon 0.4 (FLAC), lewton 0.10 (Vorbis), opus-pure 0.2.2
(Opus, then rubato 0.15 FFT 48→44.1 kHz with its output delay removed). Each decoded track is cut or padded to
`loop_end` from thbgm.fmt and compared with the original PCM (`bgmcheck`):
- **lag**: cross-correlation in ±64 frames.
- **SNR**: waveform SNR, plus lpSNR (SNR below ≈ 2.7 kHz, a 16-tap box filter). Waveform SNR understates perceptual
  codecs because they throw away HF noise on purpose. Nothing was listened to (the brief forbids playing audio).
- **seam**: the played stream around the loop, `dec[le−4096, le) ++ dec[ls, ls+4096)`, against the same splice of
  the original. Reported as seam SNR, RMS error in ±32 frames of the junction divided by the track's global RMS
  error, and the sample step at the junction compared with the original's step.

"pad" means the encoder input was `[0, le) ++ [ls, ls+1 s)`, so the encoder sees the musical continuation at the end
of the stored data. "n" means plain `[0, le)`.

| variant | total bytes | kb/s | vs 450 MB | lag | decoded length | SNR min/med | lpSNR min/med | seam SNR min | junction RMS / global RMS, max | 1-thread decode, all 42.5 min |
|---|---|---|---|---|---|---|---|---|---|---|
| raw PCM (today) | 449 961 008 | 1411 | 1× | — | exact | ∞ | ∞ | ∞ | — | read only |
| FLAC -8 | 332 957 052 | 1045 | 1.35× | 0 | exact, **bit-identical on all 21** | ∞ | ∞ | ∞ | 0 | 2.1–2.3 s (1125–1209× RT) |
| Vorbis q6 pad | 67 406 009 | 211 | 6.7× | 0 | le + 44100..44224 (pad + no end-trim) | 19.9/22.5 | 27.1/28.9 | 19.4 | 0.99 | 4.15 s (619×) |
| Vorbis q5 n | 54 950 332 | 172 | 8.2× | 0 | le + 0..64 | — | — | — | — | 4.48 s (570×) |
| **Vorbis q4 n** | **44 110 193** | **138** | **10.2×** | **0** | le + 0..64 | 14.9/17.6 | 22.7/24.3 | 14.7 | 1.21 | 3.53 s (722×) |
| Vorbis q4 pad | 44 468 058 | 139 | 10.1× | 0 | le + 44100..44224 | 14.9/17.6 | 22.7/24.3 | 14.6 | 1.40 | 3.57 s (719×) |
| Vorbis q3 n | 37 897 970 | 119 | 11.9× | 0 | le + 0..64 | — | — | — | — | 4.13 s (618×) |
| Opus 128 pad | 43 788 205 | 137 | 10.3× | **0 or 1** | via 48 kHz round trip | 11.6/14.5 | 17.2/20.2 | 10.8 | 1.24 | 6.30 s (408×), incl. resample |
| Opus 128 n | 43 425 452 | 136 | 10.4× | 0 or 1 | — | 11.6/14.5 | 17.2/20.2 | 10.8 | 1.24 | 6.36 s (401×) |
| Opus 96 pad | 32 878 977 | 103 | 13.7× | 0 or 1 | — | 10.6/13.3 | 15.9/18.3 | 9.9 | 1.09 | 5.86 s (439×) |

Per-track rows are in `results/bgm_<variant>.txt`. Findings:
- **FLAC is exact but only 1.35×.** The music is dense and loud. It is the lossless fallback (option P5 below), not a
  real size cut.
- **Vorbis: the start is sample-exact on all 21 tracks (lag 0), and the stream runs at 44.1 kHz.** `loop_start` and
  `loop_end` from thbgm.fmt therefore index the decoded samples directly. lewton applies no end-trim from the last
  granule position (up to 64 extra frames). The decoder must cut to `loop_end` from thbgm.fmt, which is logic data
  and stays byte-identical, so the loop points never come from the codec.
- **The seam adds no artifact beyond the codec's own noise.** Seam SNR is about the track's global SNR, junction RMS
  is at most 1.2–1.4× the global RMS error (noise-level variation; for many tracks it is below 1), and the junction
  step is like the original's. Padding the encoder input makes no measurable difference at q4. Keep it in the repack
  tool anyway: it is cheap and protects the last frames before `loop_end` from the encoder's end-of-stream handling.
- **Opus is the smallest (96 kb/s ≈ Vorbis q3 in bytes), but not sample-exact.** Opus is 48 kHz only. Going
  44.1 → 48 → 44.1 gave lag 0 on some tracks and 1 on others (sub-sample phase of two resamplers). Keeping 48 kHz and
  scaling the loop points by 160/147 gives a non-integer loop length (≤ 0.5 sample @ 48 kHz error per wrap). Either
  way the loop is no longer the original sample. Decoding is also 1.8× slower than lewton. The only pure-Rust decoder
  (opus-pure 0.2.2) is new, while lewton is mature and wasm-proven. **Opus rejected** under the sample-exact
  constraint.
- Decode cost: Vorbis q4 decodes the largest track (th08_15, 3 min) in 0.40 s on one thread here. Expect it to be
  several times slower on a phone (not measured: no device; the emulator is x86 and not representative). That is a
  load stall if it stays synchronous (§3.4 progressive decode).

### 2.2 Textures (306 THTX entries → RGBA8, 37.2 Mpx; `texexp`, `texbench`, `results/tex.tsv`)

Quality is the PSNR against the RGBA8 decode of the THTX data. "RGB" weights the error by the source alpha (what a
blended sprite shows); "A" is the alpha channel. Global = pixel-weighted over all textures. The worst texture is
always etama.anm#0 (bullets: sharp edges, many colours per block).

| candidate | on-disk bytes | RGB PSNR global / min / p10 / med | A PSNR global / min | decode, 1 thread, all 306 | VRAM |
|---|---|---|---|---|---|
| THTX as today (inside .anm, LZSS archive) | (part of 46.8 MB) | lossless | lossless | LZSS 0.313 s + THTX→RGBA8 0.043 s | 149.0 MB RGBA8 |
| **.anm zstd -19, per file** (THTX native 16/32-bit) | **18 736 421** (all 113 .anm incl. scripts) | lossless | lossless | zstd (C) 0.054 s, **ruzstd (pure Rust) 0.209 s** | 149.0 MB |
| .anm, zstd -19 --long, solid | 14 711 819 | lossless | lossless | — | 149.0 MB |
| PNG (best, adaptive) of RGBA8 | 29 067 098 | lossless | lossless | 0.161 s | 149.0 MB |
| WebP lossless | 25 483 630 | lossless | lossless | 0.231 s | 149.0 MB |
| WebP lossy q90 (yuva420) | 8 716 316 | 36.7 / 24.6 / 31.7 / 36.2 | 55.3 / 48.1 | — | 149.0 MB |
| WebP lossy q75 | 5 750 844 | 34.5 / 24.2 / 29.9 / 33.5 | 55.3 / 48.1 | — | 149.0 MB |
| BC7 (ISPC alpha slow) | 37 240 832; zstd 16 258 824 | 42.5 / 30.4 / 37.5 / 41.2 | 40.8 / 27.4 | unzstd 0.023 s, upload as-is | **37.2 MB** |
| ASTC 4×4 (astcenc medium) | 37 240 832; zstd 16 632 816 | 42.9 / 32.1 / 37.3 / 42.0 | 43.1 / 30.0 | upload as-is | 37.2 MB |
| ASTC 6×6 | 16 801 616; zstd 9 169 598 | 35.8 / 25.7 / 30.5 / 34.0 | 33.5 / 20.9 | upload as-is | 16.8 MB |
| ASTC 8×8 | 9 310 208; zstd 5 542 856 | 31.7 / 21.4 / 26.5 / 30.1 | 29.6 / 17.7 | upload as-is | 9.3 MB |
| ETC2 RGBA8 (ETC1-subset RGB + EAC alpha*) | 37 240 832; zstd 10 503 195 | 36.0 / 28.9 / 31.0 / 34.6 | 48.9 / 35.1 | upload as-is | 37.2 MB |

\* No ETC2 encoder in the toolchain: the colour half is ISPC ETC1 (valid ETC2, but no T/H/planar modes, so a lower
bound for ETC2 quality). The alpha half is a brute-force EAC encoder in `texexp.rs`, checked by decoding with
texture2ddecoder. The ISPC ASTC path of intel_tex_2 0.5 is `unimplemented!()`, so ASTC uses astcenc (ctt-astcenc
0.6.1).

Findings:
- **No new lossless texture format is needed.** Formats 3 and 5 are 16-bit. Expanding them to RGBA8 for PNG or WebP
  doubles the input, so both end up larger than zstd of the untouched `.anm` files. That zstd output also carries the
  scripts and sprites, and keeps the file byte-identical.
- **Lossy WebP saves ≈ 6–9 MB of disk at a clear quality cost.** Bullets at 24.6 dB, and yuv420 chroma subsampling
  on pixel-art sprites, fail "visually identical" (parity policy). Rejected.
- **GPU formats save no disk** (BC7/ASTC4×4 zstd ≈ 16 MB > 14.7–18.7 MB for the whole .anm set). They only cut VRAM
  4× (149 → 37 MB for the full set; one scene holds a subset, with eff09al/sk 2048² the largest at 16 MB each). The
  best ones are at 42–43 dB global but only 30–32 dB on etama, so they are not identical either. wgpu has no 4444/565
  formats, so lossless VRAM halving is impossible. If VRAM ever matters on a device, a per-platform GPU pack
  (BC7 desktop, ASTC 4×4 Android, none on web) is a separate option (§3.6). It is not part of this plan.
- SE: 2 155 488 B raw → FLAC 1 653 962, Vorbis q4 441 742, Opus 48k 279 771, zstd 1 787 540. The most a lossy codec
  can save is 1.9 MB (0.4 % of the APK). Keep the wav bytes unchanged and zstd them.
- MIDI 1 010 588 → zstd 188 612. jpg 4 343 794 → zstd 3 182 729: keep the bytes (re-encoding would change pixels).
  png → zstd gains nothing (store).

### 2.3 Whole-archive recompression of th08.dat content (all 317 decoded entries, unchanged bytes)

| layout | bytes | random access |
|---|---|---|
| PBGZ LZSS (today) | 46 838 025 | per entry |
| zstd -19 per file | 26 248 257 | per entry |
| zstd -19 solid frames ≤ 1 MB (files never split) | 25 480 289 | per frame |
| zstd -19 solid frames ≤ 4 MB | 23 304 177 | per frame (max frame 8.5 MB, a single big .anm) |
| zstd -19 solid frames ≤ 16 MB | 21 879 133 | per frame |
| zstd -19 --long one solid stream | 21 015 219–21 290 008 | none |
| xz -9e solid | 19 468 952 | none |
| per-piece with THTX dedupe (283 unique of 306) | 29 615 438 | — (smaller pieces lose more context than dedupe wins) |

Loading is faster than today too: all 113 .anm take 0.21 s with ruzstd against 0.31 s for the LZSS path. The whole
container in RAM is 22–26 MB instead of 46.8 MB, and with ranged reads (§3.3) only the index has to stay resident.

### 2.4 Candidate packages (arm64 personal APK; code = libth08.so 13.2 MB + dex 2.0 MB ≈ 15.5 MB, from
`target/android/th08-material-release.apk`)

| # | th08 content | BGM | SE / tex | APK ≈ | vs 512 MB | lossless? | extra decode at load (this PC) | RAM vs today |
|---|---|---|---|---|---|---|---|---|
| P0 | th08.dat as is | raw PCM | — | 512 MB | 1× | yes | — | — |
| P1 | th08.dat as is | Vorbis q4 | as is | 106 MB | 4.8× | BGM lossy | ≤ 0.40 s per track | same |
| **P2** | **zstd pack (16 MB frames)** | **Vorbis q4** | **byte-identical** | **≈ 81.5 MB** | **6.3×** | **BGM lossy only** | ≤ 0.40 s per track; .anm faster than today | −25 MB (archive) |
| P3 | zstd pack | Vorbis q3 | byte-identical | ≈ 75 MB | 6.8× | BGM lossy | ≤ 0.40 s | −25 MB |
| P4 | zstd pack | Opus 96 | byte-identical | ≈ 70 MB | 7.3× | **loops not exact** | ≤ 0.7 s incl. resample | −25 MB |
| P5 | zstd pack | FLAC -8 | byte-identical | ≈ 370 MB | 1.4× | fully lossless | ≤ 0.23 s | −25 MB |
| P6 | P2 + WebP q90 textures | Vorbis q4 | lossy tex | ≈ 77 MB (estimated: THTX share of the pack ≈ 14 MB → 8.7 MB) | 6.6× | **draw not identical** | +0.2 s | — |

Per-file zstd instead of 16 MB frames adds 4.4 MB to P2–P5. Desktop/AppImage/NSIS packages get the same data saving
(−465 MB).

## 3. Recommendation and integration plan

**Ship P2:** a repacked `th08.pak` (every th08.dat entry byte-identical, zstd) plus `thbgm.pak` (Ogg Vorbis q4 per
track, padded encode, truncated to thbgm.fmt's `loop_end` on decode). P3 is the same code with `-q 3` if 6 MB more
matter. Raw `th08.dat` / `thbgm.dat` stay supported side by side: the user's game folder and arch_check use them.

### 3.1 Container (port-only format, not a spec format; written only by the repack tool)

`th08.pak` and `thbgm.pak` use one layout:
- 32-byte header: magic `T8PK`, version u32, kind u32 (1 = th08 content, 2 = bgm), manifest offset/len u64, and
  sha256 of the manifest (first 8 bytes).
- Data: zstd frames (content pack, ≤ 16 MB solid frames grouped by extension, a file never spans frames) or Ogg
  Vorbis streams (bgm pack, one per track).
- Manifest: postcard or a tiny hand-written LE table; serde_json is also fine at 317 entries. Per entry:
  `name`, `layer` (Logic | Present), `codec` (Zstd | Store | Vorbis), `frame`, offset/len inside the decoded frame
  (or the stream's range), `size`, and `src_sha256` = sha256 of the bytes `Archive::read(name)` returns (or of the
  track's PCM `[0, loop_end)` for bgm). The bgm pack also stores the original thbgm.dat 16-byte header,
  thbgm.dat's total size and sha256, and per track `loop_end` (for checking against thbgm.fmt).
- Logic-entry hashes are compared with a vanilla table compiled into the port: the sha256 list of all 317 entries
  of the original th08.dat, generated once by the repack tool from the user's file and checked in as a constant
  table. Hashes only, no game data. If the table must not be committed, the pack holds the hashes and the check is
  "pack hashes equal the hashes of the archive it came from", done at repack time.

### 3.2 Crates (pure Rust on every target; C only in the repack tool)
- Decode: `ruzstd` 0.8 (pure Rust; 0.21 s for all .anm; works on wasm32 and Android without a C toolchain), `lewton`
  0.10 + its `ogg` (pure Rust Vorbis, sample-exact start measured), `sha2` (already in Cargo.lock) for the optional
  load-time check.
- Repack tool (desktop only): `zstd` (C, level 19 with long matching), Vorbis encoding through ffmpeg/`oggenc` as an
  external program. There is no mature pure-Rust Vorbis encoder; the tool is personal and desktop-only, so an
  external encoder is acceptable. Or `vorbis_rs` (libvorbis bindings) inside the tool. Never in the game binary.

### 3.3 Where it plugs in
- **DataSource / content (ARCH_D §2.3, ARCH_B D1).** A `Pack` reader in a new `th08-formats::pack` reads the
  manifest by range and then frames on demand through `Source::read_range`. Keep an LRU of 1–2 decoded frames.
  Startup today does `Archive::from_bytes(source.read("th08.dat"))` (`game.rs:284`). It becomes a `Content` enum
  `{ Archive, Pack }` behind `LogicView::new`, chosen by `names()` (prefer `th08.pak`, else `th08.dat`).
  `LogicView::read` and `PresentView::read` keep their signatures and return the same bytes, so **no core caller
  changes**, anmmgr included (textures stay inside the .anm bytes). The fingerprint stays vanilla (empty) when every
  Logic entry's `src_sha256` equals the vanilla table. A mismatch is treated as a Logic override: it changes the
  fingerprint and lands in `meta.txt` (ARCH_B D5). A broken repack is therefore visible, never silent.
- **thbgm header probe (02 §1, `game.rs:296`).** `read_range("thbgm.dat", 0, 16)` keeps working when only
  `thbgm.pak` exists: the BGM source answers with the stored original header (`ZWAV`, 1, 0x800, 0), so `config::load`
  makes the same decision.
- **Audio (`th08-audio/src/audio.rs:510–533` `play_wav`).** Put a `BgmSource` in front of it: `Raw` (range read of
  thbgm.dat as today) or `Vorbis` (range read of the track's Ogg stream from the pack, then lewton). Output is the
  same `Vec<[f32; 2]>`, cut or zero-padded to `t.length` = thbgm.fmt `loop_end`. `Music::new` / `track_frame` /
  the 4.0 s ring logic stay as they are, so 02 §3 behaviour is untouched. SE and MIDI read unchanged bytes through
  `PresentView`.
- **Renderer: no change** for P2.

### 3.4 Load latency and RAM
- Progressive decode (second step, optional): `Music` gets a growing sample buffer. The loader thread decodes the
  first ring (4.0 s ≈ 0.01 s of CPU) and publishes it, then appends the rest. The play command fills the ring as soon
  as 4 s exist, so even a slow phone starts without the 0.4 s+ decode stall. Before the loop wraps, the whole
  `[0, le)` is decoded (decoding runs at 700× real time).
- RAM: storing samples as `[i16; 2]` instead of `[f32; 2]` halves the largest track from 63.5 to 31.7 MB. The
  transient compressed track is at most 3.2 MB, against 31.7 MB of raw PCM today.

### 3.5 Repack tool and checks
- `crates/th08-tools/src/bin/repack.rs` (desktop only): `repack <game dir> <out dir> [--vorbis-q 4]`. It writes
  `th08.pak` and `thbgm.pak` (personal output under `target/` or the packaging staging dir, never committed). It
  verifies itself: every pack entry decodes to `src_sha256`; every track decodes to exactly `loop_end` frames with
  lag 0, using the `bgmcheck` metrics from this report as asserts (seam SNR ≥ global SNR − 1 dB, junction RMS ≤ 1.5×
  global).
- `tools/android/build_apk.sh --data` and the Gradle `GameData` task (`android/app/build.gradle.kts:69–90`) accept
  the `.pak` pair (`noCompress += "pak"`). `ApkAssetSource` already does ranged fd reads, so nothing else changes.
  The AppImage and NSIS scripts get the same switch.
- **arch_check / traces stay valid:** arch_check keeps running from the original game folder (`TH08_DIR`), so its
  T/D references are untouched. Add one pack item: `session` on 2 sessions + demo 0 with `TH08_DIR` pointing at a
  folder holding only the two `.pak` files. Expect identical trace **and** draw hash, because content bytes, texture
  sizes and the BGM header are identical. Audio is not in any trace. A core unit test reads every entry through
  `Content::Pack` and `Content::Archive` and compares them (skipped without game data, like `testdata.rs`).

### 3.6 Not now (recorded so it is not re-researched)
- Opus (not sample-exact at 44.1 kHz, §2.1).
- Lossy textures (fail visually identical).
- GPU texture packs: VRAM only, platform-specific. If ever wanted, they need `PresentView::anm_textures(name) ->
  Vec<Option<TexData>>` with `TexData { Rgba8 | Bc7 | Astc4x4 }`, `ResCmd::Create` gaining a format, and every
  texture that is a destination of `Text` / `Write` / `Copy` / `CaptureRect` kept RGBA8 (front.anm boss plate,
  text/music/result/capture targets). This supersedes ARCH_D §2.1 "Not pursued: transcoding thbgm.dat" for the BGM
  only.

### 3.7 Runs (≤ 3 deliverables each), in order

| run | deliverables | files | order / check |
|---|---|---|---|
| **AS1** pack format + repack — **DONE 2026-11-04** (see "AS1 result" below) | (1) `th08-formats::pack` reader/writer (header, manifest, zstd frames via ruzstd decode / zstd encode behind a tool-only feature) with unit tests; (2) `repack` bin: th08.pak from th08.dat + self-verify; (3) vanilla Logic sha256 table + `Content::{Archive, Pack}` under `LogicView::new`, fingerprint rule | formats/pack.rs (new), tools/bin/repack.rs (new), core/content.rs, game.rs:284 (one line) | **after 11p** (11p owns drawlist/anmmgr/game load paths and refreshes the D reference). Check T D on the normal set + the new pack item (identical) |
| **AS2** BGM pack — **DONE 2026-11-04** (see "AS2 result" below) | (1) thbgm.pak writer in `repack` (padded encode, Vorbis q4, header + loop table, self-check of lag/seam/length); (2) `BgmSource { Raw, Vorbis }` in th08-audio, lewton decode cut to `loop_end`, header probe answered from the pack; (3) samples as `[i16; 2]` | tools/bin/repack.rs, audio.rs, game.rs:296 (probe via source) | after AS1; ∥ the Android runs that do not touch audio.rs. Check T D identical (pack item); bgmcheck metrics in the repack self-check |
| **AS3** packaging — **DONE 2026-11-04** (see "AS3 result" below) | (1) build_apk.sh / Gradle `GameData` + `noCompress "pak"`, prefer `.pak`, launcher folder check accepts `.pak`; (2) AppImage / NSIS staging use the packs; (3) progressive BGM decode (growing buffer, first ring first) | tools/android/*, android/app/build.gradle.kts, packaging scripts, src/sources.rs / launcher folder check, audio.rs | after AS2; Android emulator only (headless, `adb -s emulator-NNNN`): install, start, title BGM starts (logcat "bgm" timing), APK size recorded |

Ordering against step 11: the plan only needs `LogicView` / `PresentView` (already in place) and the one-line load
in `game.rs:284`. It does not depend on 11p's image work, but it touches `game.rs` and `audio.rs`, which 11j/11o
own. Running it after 11p avoids merge conflicts and keeps the 11p D refresh clean. If the APK size is needed
sooner, AS1 + AS2 can run right after 11o while 11p is pending: they touch neither `drawlist.rs` nor `camera.rs`. AS3
comes after the current Android runs (launcher/Compose, safe area, touch) have landed.

**Schedule (orchestrator, 2026-11-04):** AS1 starts right after step-11 wave 1 is merged (needs 11b's formats work), in
parallel with wave 2 (only `game.rs` one-line overlap with 11j); AS2 right after AS1 (audio crate, no step-11 run touches
it before 11o); AS3 right after AS2, then the small personal APK. 11p later refreshes the D reference as planned.

### AS1 result (2026-11-04)

**Files.** `crates/th08-formats/src/pack.rs` (new: `Pack` reader, `write`, `layer_of`, `sha256`; deps `ruzstd` 0.8 +
`sha2` 0.10, pure Rust); `crates/th08-formats/src/archive.rs` (`ArchiveError::Pack` variant);
`crates/th08-tools/src/bin/repack.rs` (new; `zstd` 0.13 C encoder, tool only); `crates/th08-core/src/content.rs`
(`Content::{Archive, Pack}`, `Content::load`, pack fingerprint rule); `crates/th08-core/src/vanilla.rs` (new: 217
Logic-entry sha256s, generated by `repack --table`); `game.rs` load line (+ the now unused `Archive` import removed).

**API.** `Pack::from_bytes(Vec<u8>)`, `read(name)` (case-insensitive, first read of each entry checked against its
manifest sha256), `names()` (th08.dat table order), `entries()`, `frames()`, `entry(name)`, `kind()`.
`pack::write(kind, &[(name, bytes)], frame_limit, &mut compress)` (writer takes the compressor, so formats has no C
dependency; tests use ruzstd's own encoder). Core: `Content::load(&Source)` prefers `th08.pak`, else `th08.dat`;
`LogicView::new(impl Into<Content>)` (an `Archive` still converts, so other callers are unchanged).
`repack <game dir> <out dir> [--frame-mb N] [--level L] [--table FILE]`, default 4 MB frames, level 19, checksum on.

**Fingerprint rule.** th08.dat → vanilla (empty), as before. th08.pak → vanilla iff its Logic entries are exactly
the table (same names, same sha256, Logic layer); otherwise sha256 over the sorted differing names, each followed by
its pack hash (none when missing). Unit-tested: original repack → vanilla; one `.ecl` changed → non-vanilla; only a
`.jpg` changed → vanilla (Present layer).

**Sizes (personal output `target/pack/th08.pak`, deterministic).** th08.dat 46 838 025 B (unpacked 112 516 076 B).
4 MB frames: **23 338 662 B**, 40 frames, largest 8 520 492 B (one .anm). 16 MB frames: 21 908 508 B, 20 frames.

**Load time / RAM** (release, this PC under load ~15–20, `/usr/bin/time`): s_selftest1 alternated 3×: th08.dat 1.00 /
0.93 / 0.95 s, 447 MB peak; th08.pak (4 MB) 0.97 / 0.96 / 0.98 s, 437 MB peak. 16 MB frames cost about +0.4 s per
process and +45 MB peak (each cache miss decodes 16 MB; 2 frames cached). golden: dat 25.1 / 22.0 s, pack 24.0 s (noise).

**Proof (pack item, manual).** Dir `target/pack/game/` with only `th08.pak` and a symlink to the original `thbgm.dat`;
arch_check's `run_item` (full: trace, report, draw hash) run with `TH08_DIR` set to it for s_golden, s_selftest1,
d_0 and p_gensokyou_th8_udAr02: all four digests equal `target/arch_ref/digests.txt` (16 MB and 4 MB packs both).
A permanent arch_check item can reuse that: `TH08_DIR=<pack dir>` for those items, compare with the same reference.

**Deviations.** (1) Default frames 4 MB, not 16 MB (+1.4 MB file, but start-up equal to th08.dat and lower RSS).
(2) Header: manifest length u32 (32 bytes did not fit offset + length u64 + 8-byte hash). (3) Codec is per frame,
not per entry (an entry inherits its frame's); a frame that does not shrink is stored. (4) Manifest is a hand-written
LE table (layout in pack.rs). (5) No ranged reads yet: the whole pack (23 MB) is read into memory, frames decode on
demand with a 2-frame cache; `Source::read_range` streaming can come with AS3 if Android RAM needs it.

### AS2 result (2026-11-04)

**Files.** `crates/th08-formats/src/pack.rs` (`Codec::Vorbis`, `Index` = header + manifest parsed without the frame
data, `BgmInfo` trailer, `BgmTrack` + `write_bgm`, `Pack::stored`/`index`); `crates/th08-core/src/content.rs`
(`BGM_PACK`, `bgm_pack_index`, `bgm_header`); `game.rs` probe line (`content::bgm_header(&source)`);
`crates/th08-audio/src/bgm.rs` (new: `BgmSource { Raw, Vorbis }`, `decode_vorbis`, `pcm_frames`, `fit`; dep
`lewton` 0.10, pure Rust); `audio.rs` (`Music` samples `[i16; 2]`, BGM source resolved on the first play command);
`crates/th08-tools/src/bin/repack.rs` (`--only content|bgm`, `--vorbis-q Q`, `--ffmpeg PATH`; `mod bgm`).

**Format.** Same T8PK layout, kind 2. One `Vorbis` frame per thbgm.fmt track in fmt order (`size` = decoded PCM bytes
before the cut); per entry name = fmt name, layer Present, `size` = `loop_end`·4, sha256 of thbgm.dat PCM
`[0, loop_end)`. The manifest ends with the thbgm.dat trailer: original 16-byte header, file length, sha256.

**API.** Core: `content::bgm_header(&Source) -> Option<Vec<u8>>`: the stored header when a readable `thbgm.pak` exists,
else `read_range("thbgm.dat", 0, 16)` as before (a broken pack logs and falls back). `content::bgm_pack_index`
reads only the 32-byte header and the manifest by range. Audio: `BgmSource::resolve(&Source)` (the same choice) and
`read(&Source, &Track) -> Vec<[i16; 2]>`: Raw = range read of thbgm.dat; Vorbis = range read of the track's Ogg stream,
lewton, cut / zero-padded to `t.length` (thbgm.fmt `loop_end`). The loop points come only from thbgm.fmt, and
`Music::new` / `track_frame` / the ring are unchanged.

**Encode.** `[0, loop_end) ++ [loop_start, loop_start + 1 s)` → ffmpeg libvorbis `-q:a 4`, `-fflags +bitexact
-flags:a +bitexact -map_metadata -1`. The output is byte-deterministic (two runs, same sha256 `6d575df2…`). 8 parallel
ffmpeg jobs: 12.6–18 s for all 21 tracks, including the checks.

**Sizes.** thbgm.dat 449 961 024 B → **thbgm.pak 44 469 772 B (10.1×)**, personal output
`target/pack/thbgm.pak`.

**Loop metrics (repack self-check, all 21 tracks).** Decoded length = loop_end + 44100..44224 (the pad plus no
end-trim, as in §2.1), cut to loop_end. **Lag 0 on all 21.** Global SNR 14.90–21.42 dB. Seam SNR 14.61–26.34 dB.
Junction RMS / global RMS error ≤ **1.40** (th08_20, limit 1.5). The junction sample step is like the original's.
These match the §2.1 "Vorbis q4 pad" row. Every track is then read back from the written file through `BgmSource`
(the game path): frames identical to the checked decode. The header probe answers the original 16 bytes.

**Decode time.** All 21 tracks read back + decoded on one thread: 8.0–9.6 s, slowest track 0.65–0.75 s (th08_11 /
th08_14). The machine was at load 33–42 during these runs, against 3.6 s for §2.1 on a quiet machine. Peak RAM per
track is now `loop_end`·4 B as `[i16; 2]` (th08_15: 31.7 MB, was 63.5 MB as f32) plus the ≤ 3.2 MB Ogg stream.

**Proof (pack item).** Dir `target/pack/game_bgm/` holds only `th08.pak` + `thbgm.pak` (no .dat). arch_check's
`run_item` (full: trace, report, draw hash) with `TH08_DIR` = that dir for s_golden, s_selftest1, d_0 and
p_gensokyou_th8_udAr02: all four digests equal `target/arch_ref/digests.txt`. Unit tests: `pack::bgm_pack_roundtrip`
(writer / reader / manifest-only index); `content::bgm_header_probe` (pack header wins, dat fallback, broken-pack
fallback, missing → None, content pack under the BGM name rejected); the audio ring tests now run on s16 frames.

**Deviations.** (1) Self-check asserts: decoded length ≥ loop_end, lag 0, junction ≤ 1.5. Dropped: "seam SNR ≥ global
SNR − 1 dB", because it fails per track on content alone (e.g. th08_07: seam 15.8 dB vs global 20.6 dB). An 8192-frame
window's SNR follows the local loudness, and the seam splice does not change a single sample. A local bound (the same
splice 8× wider) also fails on content (th08_07, th08_19, th08_20). Seam SNR and that local SNR stay in the printed
report. (2) No lpSNR (it was only a diagnostic in §2.1). (3) Vorbis is not decoded inside `th08-formats`:
`Pack::read` on a Vorbis frame returns a `Decode` error, and the audio crate decodes. (4) BGM decode is still
synchronous per play command on the loader thread (progressive decode is AS3).

### AS3 result (2026-11-04)

**Files.** `xtask/src/assets.rs` (`--assets packed`: repack cached in `<target>/pack/` with `dist.stamp`),
`xtask/src/util.rs` (`personal_target`, `TH08_TARGET`), `xtask/src/android.rs` (packed allowed),
`android/app/build.gradle.kts` (`gameFiles` prefers the `.pak` pair, `noCompress += listOf("dat", "pak")`),
`LauncherActivity.kt` (`checkData`: th08.pak or th08.dat, two lines), `tools/android/build_apk.sh` (`--packed DIR`),
`tools/android/run_debug.sh` (`PACKED=DIR`), `crates/th08-core/src/content.rs` (`CONTENT_PACK`, `CONTENT_FILES`,
`has_content`), `src/launcher.rs` (`is_game_dir` / `folder_ok` accept th08.pak; test `pack_folder_is_game_dir`),
`src/sources.rs` (`ApkAssetSource::new` accepts th08.pak), `crates/th08-audio/src/bgm.rs` (`Pcm`, `PcmWriter`,
`BgmSource::stream`, `decode_vorbis_into`), `audio.rs` (`Music` over `Arc<Pcm>`, progressive `play_wav`).

**Packed staging.** `cargo xtask dist --target T --assets packed [--data DIR]` runs `repack` into
`<target>/pack/.dist-tmp`, renames `th08.pak` / `thbgm.pak` into `<target>/pack/` (so links like `pack/game_bgm/`
stay valid) and writes `dist.stamp` = path, size, mtime of th08.dat / thbgm.dat + sha256 of `repack.rs`,
`formats/pack.rs`, `audio/bgm.rs`. A matching stamp reuses the packs ("reusing … (dist.stamp matches)"). AppImage
(`usr/share/th08/game/`), windows-exe (`game/`), NSIS (`game\`) and Android (Gradle `GameData` assets) stage the two
packs through the same `Game::copy_to` / `-Pth08.data` paths as raw. The rebuilt packs are byte-identical to AS1/AS2
(th08.pak `409152e9…`, thbgm.pak `6d575df2…`). `--assets none` is untouched: `prepare` still returns `None` before
any game-data code, and Gradle without `th08.data` gets no assets.

**Progressive BGM.** `Pcm` = the track's `[0, loop_end)` frames in 11025-frame blocks (one ring chunk, 02 §3), each a
`OnceLock`, published in order by one `PcmWriter`; the mixer reads without locks, and a block not decoded yet reads
as silence and sets `underrun` (logged by the loader at the end). `play_wav` creates the `Music` (fill stage
unchanged) as soon as the first ring (16 blocks = 4.0 s, or the whole shorter track) is published, then keeps
appending. Decoding stops when the generation changes before the start, or when the Music's `Arc` is dropped (new
play / release). Raw thbgm.dat reads the first ring by range, then 1 MiB ranges. Loop points, `track_frame`, ring
and stop/unpause logic are unchanged (ring tests pass on `Pcm`). New unit tests: `pcm_writer_blocks_cut_and_pad`,
`pcm_writer_stops`. repack's read-back (all 21 tracks through `BgmSource::read` → `stream`) = checked decode.

**Sizes (personal, `target/dist/`).**

| package | bytes | raw-assets build |
|---|---|---|
| android-apk arm64-v8a, `-assets-packed.apk` | 83 051 153 (79.2 MiB) | 511 471 881 |
| android-apk arm64-v8a + x86_64 (emulator test) | 99 196 649 (94.6 MiB) | — |
| linux-appimage `-assets-packed.AppImage` | 76 503 544 (73.0 MiB) | — |
| windows-nsis `-assets-packed.exe` | 75 961 805 (72.4 MiB) | — |

APK entries `assets/th08.pak` / `assets/thbgm.pak` are Stored (unzip -v).

**Emulator (headless th08dbg, `adb -s emulator-5584`, x86_64, SwiftShader).** Release APK (keystore via xtask)
installed and started; Compose launcher shows "Game data: Included in this app"; Play → logcat `game files bundled in
the APK: ["settings_schema.json", "th08.pak", "thbgm.pak"]`, `APK asset th08.pak … thbgm.pak` opened as fds. **BGM
start latency** (play command → Music playing): title `th08_01.wav: first 176400 frames after 7.4 ms`, whole track
after 190.1 ms; demo `th08_11.wav`: 12.0 ms vs 541.9 ms for the whole track. No underrun logged. The title and
then the stage 5 demo ran.

**Deviations.** (1) `TH08_TARGET` (env) names another checkout's `target/` (must be named `target` next to a
Cargo.toml) for dist outputs, the pack cache and the game-data guard, so a worktree build writes to the main
checkout's target/. (2) The cache lives directly in `target/pack/` (not `target/dist/packs/<name>`), shared by all
targets. (3) Desktop BGM latency not measured in the game window: there is no mute switch and the run would open the
real audio device; desktop uses the same code path. (4) Messages / i18n strings still say "th08.dat" (L2 owns
i18n). (5) wasm keeps decoding on the page's thread (spawn_local, as before), now with the ring published first.
