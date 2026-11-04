# Step 11 — idiomatic pass (plan)

Design: ARCH_A_IDIOMATIC.md §2.4 (types), §2.5 (VM `Op` enums), §4 step 11, §5 (D5 indices, D6 one decode style),
ARCH_SUMMARY decisions 4 / 5 / 10, REWRITE_PLAN step 11 (T D per module, B weekly), STEP9_WORLD.md / STEP10_CRATES.md
(the crates as built), CLAUDE.md (spec-citation comments stay). Measured on master e45217e (workspace as cut in 10c).
Rule: no behaviour change; arithmetic, float op order, call order and RNG draw order frozen; every run passes
`tools/arch_check run target/arch_ref` (T + D) and the run's clippy-subset count for its files drops to 0.

## 0. Measurement (how the counts were taken)

- clippy: `flock target/.agents.lock cargo clippy --workspace --all-targets -- -W clippy::pedantic` → **3 324**
  warnings (deduplicated by file:line:col); default lints alone → **111**. Top pedantic: `cast_possible_truncation`
  457, `cast_sign_loss` 451, `cast_lossless` 414, `must_use_candidate` 368, `unreadable_literal` 359,
  `cast_precision_loss` 249, `cast_possible_wrap` 193, `default_trait_access` 108, `many_single_char_names` 90,
  `too_many_lines` 42 (> 100), `float_cmp` 34, `too_many_arguments` 12, `struct_excessive_bools` 12.
- **Subset** (lints that are worth fixing in a bit-exact port; §3): `cast_lossless`, `cast_possible_wrap`,
  `cast_sign_loss`, `unreadable_literal`, `too_many_lines` (threshold 150), `too_many_arguments` (threshold 8),
  `struct_excessive_bools`, `semicolon_if_nothing_returned`, `redundant_closure_for_method_calls`, `explicit_iter_loop`,
  `manual_let_else`, `items_after_statements`, `default_trait_access`, `if_not_else`, `single_match_else`,
  `needless_pass_by_value`, `match_same_arms`, `bool_to_int_with_if` + all default lints → **1 786** today.
- **Not** in the subset (kept as `allow`, reason): `cast_possible_truncation` / `cast_precision_loss` (`f32 as i32` is
  the original's ftol, `i32 as f32` its fild; Rust `as` saturates, any helper must keep `as` inside), `float_cmp`
  (exact compares are the original's), `many_single_char_names` (spec formulas), `must_use_candidate`, `missing_*_doc`,
  `doc_markdown`, `module_name_repetitions`.
- Raw numbers: `as` casts to a primitive (grep `as (u8|…|f64)\b`), hex literals in code (comments stripped), `F_*` flag
  consts, `bool` fields, `Option<usize>` / `usize` slot handles, `.unwrap()` before the test module.
- SPEC-GAP (tools/metrics.py rule): **1** (`crates/th08-render/src/text.rs:3`, GDI font, permanent). No dead SPEC-GAP
  code paths remain; no `#[allow(dead_code)]` in core. "Dead paths" in step 11 = the 3-line re-export shims and the
  duplicated helpers below.

## 1. Per module group: today → target

Counts: pedantic / subset / casts (all five cast lints) per group; files listed are the group's ownership for §2.

### 1.1 Core VMs — anmvm, anmmgr, eclvm, enemy, stage, msg (pedantic 620, subset 367, casts 413)

| module | not idiomatic today (file:line) | target |
|---|---|---|
| ANM (anmvm, anmmgr) | `Ins::read` decodes header + 8 raw `u32` args from bytes on every step (anmvm.rs:228–260), opcodes as a 94-arm `match ins.op` with positional `a[k]` (646), sub-matches on `ins.op` inside arms (900–980); `AnmVm::run` 441 lines (608); 15 `F_*: u32` flag consts (anmvm.rs:7–32), 23 flag ops; `color: [u8; 4]` BGRA (300–330, byte order comment 43); `wrap_add` with a local `TWO_PI` (122); 69 `as`, 59 hex | `th08_formats::anm::{AnmOff(u32), Ins}` (header decoder moves to formats, lazy per ip, D6); `anm::Op` enum from `Op::decode(&Ins)` with named fields, `Op::Unknown(i16)`; `run` = loop + `exec_op` + per-group fns (`ops_flow`, `ops_interp`, `ops_sprite`, `ops_var`) each arm `// 05 §5 op N`; `VmFlags` (bitflags, same bits); `D3dColor` |
| ECL (eclvm) | `Exec::exec` 858 lines, 197 numeric arms (eclvm.rs:1563); args by position `self.ai(t, ins, 2)` / `af` / `set_i` (472–600); var ids decoded in `int_ref` (155, `IRef` already an enum); literal bullet flag `0x100000` (1231, 1386); second `TWO_PI` (1415); 150 `as`, 82 code hex; `fn` at 1143 167 lines | `th08_formats::ecl::EclOff(u32)` for `Ins.off` (formats ecl.rs:34, 49); `ecl::Op` enum (`Wait(Arg)`, `Jump { to: EclOff, .. }`, `SetI(Dst, Arg)` …) decoded lazily (the file stays writable bytes, 9c); `Arg` = raw-or-var, read through `Var` (`int_ref` → `Var::from_id`) with RNG-drawing reads taking `&mut Rng`; arms grouped into `eclvm/ops_{flow,move,bullet,laser,spell,native}.rs` |
| ECL (enemy) | `Enemy` 23 bool fields (enemy.rs:213, `struct_excessive_bools` at 213 / 614 / 643); chain / boss links `Option<usize>` (322–324, 337, 627, 686); timeline 21-arm `match ins.op` (1139); fns 1133 (105), 1392 (122); 78 `as` | `EnemyIdx(u16)`; bool groups that are one flag word in 07 → `EnemyFlags` bitflags (others stay bool); `TlOp` enum for timeline ops; long fns split by spec step |
| STD (stage) | `Record.i(k)` / `f(k)` positional (formats stdfile.rs:66–70), 32-arm `match r.op` (stage.rs:475); `Stage` bools (247); 43 `as` | `std::Op` enum from `Record` (formats); arm per op with `// 06 §1.4` |
| MSG (msg) | 49-arm `match ins.op` (msg.rs:384) in a 139-line fn (375); `Dialogue` bools (122); `Ins.at: usize` (formats msg.rs:23–28) | `msg::Op` enum; `MsgOff`; exec split per op group (text, portrait, flow) |

### 1.2 Gameplay — player, pshot, bullet, item, bomb, effect (pedantic 573, subset 355, casts 245)

| module | today | target |
|---|---|---|
| player, pshot | `shot_type: u8` compared as numbers (`matches!(self.shot_type, 1 \| 6 \| 7)` pshot.rs:489, 507; item.rs:470); `Player` bools (player.rs:365); `angle_from` duplicated (player.rs:668, bullet.rs:81); pshot fn 260 (143 lines), 887 (8 args); 179 / 112 hex | `ShotType` `#[repr(u8)]` enum + `team()` (sht.rs) and named predicates per spec row; `Angle` for player/shot angles; one `angle_from` in `types` |
| bullet, item | bullet flags `u32` with `F_NO_CANCEL` and literals (bullet.rs:33, 18 flag ops), laser `flags: u16` (369); slot handles `usize` (`live` 410); fn 657 (129), 570 (8 args); 37 unreadable literals | `BulletFlags` / `LaserFlags` bitflags; `BulletIdx`, `LaserIdx`, `ItemIdx` (fixed arrays, original slot order, D5); `Angle`; env struct for the 8-arg spawn |
| bomb | 75 unreadable literals, 131 code hex (sprite ids, colours, per-character tables), `too_many_arguments` 348 / 1024, fn 1067 (113) | named consts with spec rows, `D3dColor`, env struct `BombEnv` |
| effect | `struct_excessive_bools` (effect.rs:69); four 8–9-arg fns (495, 518, 552, 1152); fns 314 / 568 / 1523 | `EffectIdx`; one `FxEnv` per call family; state bools → enum where they are a mode |

### 1.3 Flow — gamemgr, replay, game, world, chain, task, supervisor, result, ending, musicroom, trace, sound, input, rng, fmath, camera, drawlist, drawfmt, host, config (pedantic 641, subset 361, casts 340)

| module | today | target |
|---|---|---|
| gamemgr | scenes as `pub const SCENE_*: i32` (gamemgr.rs:96–106) used 123× in 15 files (supervisor 32, gamemgr 30, ascii 13, menu 13, game 9, result 8, tools 9, app 1); `difficulty: i32`, `shot: u8`, `stage: usize`, `flags: u32`, `paused: u8`, `game_over: u8`, `loading: u8` (112–134); flag bits + two 2-bit fields by shift (270–304); fns 713 (157), 1078 (166); `manual_is_multiple_of` ×7, `manual_range_contains` ×3 | `SceneId` `#[repr(i32)]` (+ `TryFrom<i32>`; trace / draw-log columns keep `as i32`); `Difficulty`; `GameFlags` bitflags with `end()` / `route()` accessors for the 2-bit fields; `Paused { No, Open, Drawn }` (11 §2: 1 menu, 2 drawn once), `Loading` enum; long fns split by 11 §4 step |
| supervisor, result, ending, musicroom | `Box::new(Default::default())` ×3 (`box_default`); `ResultScreen` bools (result.rs:96); fns result 340 (120), 1614 (169), supervisor 444 (109); 89 `as` in result | `SceneId`; result state machine enum (16 §2 states) instead of bool set; split per state |
| replay, game, world, chain, task, trace | `manual_is_multiple_of` ×3 (replay); `game.rs` 6 unwraps; `trace.rs` 26 `cast_lossless` | `From` conversions; unwraps → `let … else` with the documented fallback (A §2.6) |
| drawlist, camera | `image::load_from_memory` for .jpg size (drawlist.rs:697, STEP10 risk 7); `camera.rs:2–4` `TODO(step 10)`: std `sin_cos` / `tan` under `allow(disallowed_methods)` (53–152), same in menulist.rs:2, 504 | size from the JPEG / PNG header (pure `th08_formats::img::dims`), pixels decoded by the renderer (`ResCmd::CreateImage { id, bytes, w, h }`); camera + pie on `fmath` (draw-only change, run 11p) |
| shims | core `anm.rs`, `archive.rs`, `ecl.rs`, `sht.rs`, `stdfile.rs` = 3-line `pub use th08_formats::…` (45 `crate::{anm,…}::` uses in core, 11 `th08_core::{…}::` in tools / app) | removed; `th08_formats::` paths |

### 1.4 UI — hud, gui, ascii, menu, menulist (pedantic 542, subset 337, casts 337)

| module | today | target |
|---|---|---|
| hud, gui | 14 `.unwrap()` outside tests (hud.rs); 38 unreadable literals, 96 code hex; fns 339 (128), 950 (109), 1297 (105) | `let … else` with spec'd fallback; named layout consts (12 §… rows); split per panel |
| ascii, menu, menulist | `percent_glyphs` twice: ascii.rs:912 (`Option<&mut Recorder>`) and menulist.rs:421 (`each` callback), same body (12 §5.9 `Ascii_DrawGaugePercent`); ascii fns 1035 (9 args, 137 lines), 1322 (11 args, 122 lines); menulist 111 casts, 26 unreadable literals, fns 776 / 1316; menu fn 539 (144), 22 flag ops | one `ascii::percent_glyphs(…, each: impl FnMut(&AnmVm))`, the recorder form a wrapper; `MenuEnv` for the 9/11-arg fns; `D3dColor`; `SceneId` |

### 1.5 formats (pedantic 237, subset 76, casts 93)

`chunks_exact` with constant size ×7 (anm.rs:85–112, replay.rs:92, thbgm.rs:15) → `as_chunks`; `derivable_impls`
(scoredat.rs:315); `collapsible_if` (archive.rs:206); ad-hoc `u32_at` / `i16_at` readers per file (ecl.rs ~60, msg,
replay 19 reads, cfg) and 30 raw slice ranges in scoredat.rs; 17 `unwrap` in archive.rs. Target: one `Reader` cursor
(`u8/u16/i16/u32/i32/f32_le`, `at(off)`, typed `Truncated { at }` error) shared by all parsers; offset newtypes per
format (`EclOff`, `AnmOff`, `MsgOff`), `Op` decoders next to `Ins` (formats is pure, core imports them).

### 1.6 render (pedantic 140, subset 65) · audio (98 / 43) · app (168 / 34) · tools (191 / 67)

| crate | today | target |
|---|---|---|
| render | gfx.rs:264 fn 239 lines (draw-command executor); `unnecessary_unwrap` ×2; text.rs `precedence`, `derivable_impls`; **device loss**: `detach` / `attach` keep the device (gfx.rs:525–541), textures `Vec<Option<Texture>>` (117) are not rebuilt on a lost device (ARCH_D_MOBILE:183, STEP8_GFX_INVENTORY provenance `Bytes` / `Image` / `Capture`) | executor split per `DrawCmd` family; provenance table `TexId → {Bytes, Image, Capture}` filled by `execute_res`, `rebuild()` on device loss (captures come back black, accepted) |
| audio | wasm: unused `Context`, `Result`, `Arc` imports (midi.rs:4–5, `cargo check --target wasm32`); `manual_map`, `chunks_exact` ×2, `field_reassign_with_default` ×2 (audio.rs); 1 `allow(dead_code)` | imports under the same `cfg` as their users; lints fixed |
| app | `struct_excessive_bools` app.rs:69, settings.rs:161, platform/touch.rs:365; 5 `allow(dead_code)` in app.rs | after L2 (`Entry` metadata, typed `Value`s): bool groups → enums where they are a mode |
| tools | `useless_conversion` ×10 (session.rs 3, demosync.rs 3, bin/drawrun.rs 1, bin/session.rs 1, bin/demosync.rs 1, drawlog.rs 1), `type_complexity`, `redundant_closure`, `collapsible_if`, `cloned_ref_to_slice_refs`; bins session.rs:15 (196 lines), demosync.rs:7 (174), drawdiff.rs:9 (159) | lints fixed; bin `main`s split into parse / run / report |

### 1.7 Shared types (core `types.rs`, ARCH_A §2.4)

`Angle(f32)` `#[repr(transparent)]` (plain f32 ops; each spec'd normalisation a named fn: `wrap_add` (05, anmvm.rs:122),
the ECL one (eclvm.rs:1415), `>= PI` vs `> PI` sites separate), `D3dColor(u32)` with `a() r() g() b()` and
`from_bgra([u8; 4])`, `BulletIdx` / `LaserIdx` / `EnemyIdx` / `ItemIdx` / `EffectIdx` / `AnmSlot(u8)` / `SeId(u8)`
(`TexId` exists, drawlist.rs:122), `SceneId`, `Difficulty`, `ShotType`, `bitflags` (new core dep, pure). `Timer`
stays (anmvm.rs:51). No `Score10`: one scale is used per site. No generational arenas (D5).

## 2. Runs

Check per run: `tools/arch_check run target/arch_ref` full (T + D; in a worktree `FAST=1` by the agent, full by me
after merge, never two full checks at once), `tools/spec_refs.py` 0, SPEC-GAP count 1, `cargo test --workspace` count
not lower, and `cargo clippy -p <crate> --all-targets` with the subset: **0 for the run's files**. B (ref500 490/490 +
ref_b2 498/498, systemd) after the marked runs (≈ every 4). Ownership: a run edits only its files; a newtype crossing
into a file it does not own stays raw there (`.0` / `From` at the boundary) and is converted by that file's run.

| run | content (≤ 3 deliverables) | files | parallel / conflicts | check |
|---|---|---|---|---|
| **11a** wall + types — **DONE** (see below) | (1) `[workspace.lints.clippy]` subset at `warn` (+ `[lints] workspace = true` per crate), clippy.toml `too-many-lines-threshold = 150`, `too-many-arguments-threshold = 7` (orchestrator: 7, so the 8-argument fns of §1.2 count); `tools/lint_subset` prints subset counts per file; (2) core `types.rs` (§1.7) + `bitflags` dep, `wrap_add` and the ECL `TWO_PI` moved there (callers unchanged); (3) remove the 5 core shims (§1.3), paths → `th08_formats::` | Cargo.toml (all), clippy.toml, tools/lint_subset (new), types.rs (new), lib.rs (core), anmvm/eclvm (2 lines), 45 core `use` lines, 11 in tools/app | **alone, first** (touches imports everywhere) | T D |
| **11b** formats — **DONE** (see below) | (1) `Reader` cursor + typed `Truncated { at }` in cfg, msg, replay, scoredat, sht, stdfile, thbgm, archive, end, musiccmt (not anm.rs / ecl.rs); (2) `MsgOff`, `std::Op` decoder (06 §1.4) beside `Record`, `msg::Op` decoder (10 §2) beside `Ins`; (3) formats subset 0 (as_chunks, derivable, collapsible, unwraps → errors) | crates/th08-formats except anm.rs, ecl.rs | ∥ 11c 11d 11g | T D, formats unit tests |
| **11c** ANM VM | (1) `anm::{AnmOff, Ins, Op}` in formats/anm.rs (header decoder from anmvm.rs:237, lazy per ip); (2) `AnmVm::run` → loop + `exec_op` + op-group fns, `VmFlags`, `D3dColor` for `color` / `color2` / `color_start`; (3) anmmgr subset 0 | formats/anm.rs, anmvm, anmmgr | ∥ 11b 11d 11g | T D |
| **11b** formats | (1) `Reader` cursor + typed `Truncated { at }` in cfg, msg, replay, scoredat, sht, stdfile, thbgm, archive, end, musiccmt (not anm.rs / ecl.rs); (2) `MsgOff`, `std::Op` decoder (06 §1.4) beside `Record`, `msg::Op` decoder (10 §2) beside `Ins`; (3) formats subset 0 (as_chunks, derivable, collapsible, unwraps → errors) | crates/th08-formats except anm.rs, ecl.rs | ∥ 11c 11d 11g | T D, formats unit tests |
| **11c** ANM VM — **DONE** (see below) | (1) `anm::{AnmOff, Ins, Op}` in formats/anm.rs (header decoder from anmvm.rs:237, lazy per ip); (2) `AnmVm::run` → loop + `exec_op` + op-group fns, `VmFlags`, `D3dColor` for `color` / `color2` / `color_start`; (3) anmmgr subset 0 | formats/anm.rs, anmvm, anmmgr | ∥ 11b 11d 11g | T D |
| **11d** ECL exec | (1) `ecl::{EclOff, Op}` in formats/ecl.rs (+ eclview bin); (2) `Exec::exec` → `match Op` dispatching to `eclvm/ops_*.rs`, named args via `Arg` / `Var` (from `IRef`), bullet flag const; (3) eclvm subset 0 | formats/ecl.rs, eclvm (+ new ops_*.rs), bin/eclview | ∥ 11b 11c 11g; before 11e | T D, **B** |
| **11d** ECL exec — **DONE** (see below) | (1) `ecl::{EclOff, Op}` in formats/ecl.rs (+ eclview bin); (2) `Exec::exec` → `match Op` dispatching to `eclvm/ops_*.rs`, named args via `Arg` / `Var` (from `IRef`), bullet flag const; (3) eclvm subset 0 | formats/ecl.rs, eclvm (+ new ops_*.rs), bin/eclview | ∥ 11b 11c 11g; before 11e | T D, **B** |
| **11e** enemy | (1) `EnemyIdx` for chain / boss / thread links; (2) `EnemyFlags` for the 07 flag words, `TlOp` timeline enum (enemy.rs:1139); (3) long fns 1133 / 1392 split, enemy subset 0 | enemy, eclvm (index boundary only) | after 11d; ∥ 11f 11h 11i | T D |
| **11f** STD + MSG | (1) stage.rs on `std::Op`, `Stage` bools; (2) msg.rs on `msg::Op`, exec split, `Dialogue` bools; (3) both subset 0 | stage, msg | after 11b; ∥ 11e 11g 11h | T D |
| **11g** player + pshot — **DONE** (see below) | (1) `ShotType` enum (+ `sht::team`) inside player / pshot; (2) `Angle`, one `angle_from` (player.rs:668 → types); (3) player / pshot subset 0 (pshot 260 split) | player, pshot | ∥ wave 1 | T D |
| **11h** bullet + item | (1) `BulletFlags` / `LaserFlags`; (2) `BulletIdx` / `LaserIdx` / `ItemIdx`, `angle_from` (bullet.rs:81) → types; (3) spawn env struct, fn 657 split, subset 0 | bullet, item | after 11g (`angle_from`); ∥ 11e 11f 11i | T D, **B** |
| **11i** bomb + effect — **DONE** (see below) | (1) bomb named consts + `D3dColor` (75 literals); (2) effect `FxEnv` (4 fns), `EffectIdx`, state enum; (3) both subset 0 | bomb, effect | ∥ 11e 11f 11h | T D |
| **11j** gamemgr + replay | (1) `GameFlags` bitflags + `end()` / `route()`; (2) `Difficulty`, `Paused`, `Loading` enums inside gamemgr / replay / game / world; (3) long fns 713 / 1078 split, flow-core subset 0 (gamemgr, replay, game, world, chain, task, trace, sound, input, rng, host, config, drawfmt) | gamemgr, replay, game, world, chain, task, trace, sound, input, rng, host, config, drawfmt | after 11a; ∥ 11e–11i | T D |
| **11k** scenes | (1) `SceneId` for every `SCENE_*` (123 uses, 15 files, incl. tools drawreport / demosync / drawlog, app 1); (2) result state enum + splits (340, 1614), supervisor `box_default` + split (444); (3) ending / musicroom / supervisor / result subset 0 | gamemgr (consts), supervisor, result, ending, musicroom, ascii, menu, menulist, game, replay, worldclone, tools, src/app.rs (1 line) | **alone** (touches UI + flow + app) | T D, **B** |
| **11l** HUD + GUI | (1) hud unwraps → `let … else`; (2) named layout consts, splits 339 / 950 / 1297; (3) hud / gui subset 0 | hud, gui | after 11k; ∥ 11m 11n | T D |
| **11m** ascii + menus | (1) one `percent_glyphs` (ascii.rs:912 + menulist.rs:421); (2) `MenuEnv` for ascii 1035 / 1322, splits menu 539, menulist 776 / 1316; (3) ascii / menu / menulist subset 0 | ascii, menu, menulist | after 11k; ∥ 11l 11n | T D |
| **11n** render | (1) gfx.rs executor split (264); (2) device-loss rebuild from the provenance table; (3) render subset 0 (+ text.rs) | crates/th08-render | **after the current render / audio / app work lands**; ∥ 11l 11m | T D; manual desktop + Android emulator suspend / resume |
| **11o** audio + app + tools + wall | (1) midi.rs wasm imports, audio subset 0; (2) tools subset 0 (useless_conversion ×10 etc., bin `main` splits), worldclone / testdata subset 0; (3) app subset 0 after L2; arch_check gains `cargo clippy --workspace --all-targets -- -D warnings` (subset at `warn` → enforced) | crates/th08-audio, crates/th08-tools, worldclone, testdata, src/ (not settings.rs until L2), tools/arch_check | last D-neutral run; after L2 and 11n | T D W android, **B** |
| **11p** leftovers that change draw | (1) camera.rs + menulist pie on `fmath` (drop both `allow(disallowed_methods)`, the `TODO(step 10)`); (2) `image` out of core: `th08_formats::img::dims` + `ResCmd::CreateImage`, decode in gfx; (3) new D reference (`arch_check freeze` into `target/arch_ref_11p`) after my drawgroups image diff shows "visually identical" (PARITY_POLICY) | camera, menulist (2 lines), drawlist, formats/img.rs (new), gfx (CreateImage), anmmgr (load call) | **alone**, after 11o | T must be identical; D intentionally new; drawgroups image diff; B |

**11a done.** Files: Cargo.toml (workspace + root `[lints]`), crates/*/Cargo.toml (`[lints] workspace = true`; core
`bitflags = "2.13.2"`; tools `th08-formats` dep for the shim paths), clippy.toml (thresholds), tools/lint_subset (new),
crates/th08-core/src/types.rs (new: `TWO_PI`, `wrap_add`, `Angle`, `D3dColor`, `BulletIdx` / `LaserIdx` / `EnemyIdx`
/ `ItemIdx` / `EffectIdx` (u16) / `AnmSlot` / `SeId` (u8), `SceneId`, `Difficulty`, `ShotType` + `TryFrom`, tests),
core lib.rs (shim mods removed, `types` added), anmvm.rs (`pub use crate::types::wrap_add`, PI import dropped),
eclvm.rs (`use crate::types::TWO_PI`), core shims anm / archive / ecl / sht / stdfile.rs deleted, paths →
`th08_formats::` in anmmgr, anmvm, content, eclvm, enemy, game, hud, msg, player, pshot, stage, testdata, worldclone,
tools drawreport + bins anmview / datdump / eclview / stdview, audio.rs (test archive). Public API: `th08_core::{anm,
archive, ecl, sht, stdfile}` gone (use `th08_formats::…`); `th08_core::player::Sht` re-export kept; new
`th08_core::types`. Lint subset (tools/lint_subset, workspace): before 1 825 (the two moved `6.2831855` literals), after 1 823 in 71 files (plan's 1 786 was
measured on e45217e); run files types.rs / lib.rs 0. Check: FAST arch_check 0 differing, cargo test 420 passed,
spec_refs 0. Deviations: `too-many-arguments-threshold = 7` (orchestrator: 7, so the 8-argument fns of §1.2 count) as specified allows 8-arg fns, so the 8-arg sites of §1.2
(pshot 887, bullet 570) are not flagged — lower to 7 if they should count; tools / audio import lines keep their
other (11o) warnings.

**11g done.** Files: crates/th08-core/src/player.rs, pshot.rs, types.rs (additions only). (1) `Player::shot() ->
ShotType` over the raw `shot_type: u8` (kept raw: item / enemy / bomb / hud / worldclone read it, converted by their
runs); every numeric shot test in player / pshot is a `ShotType` match or predicate (`is_solo`, `is_odd`,
`is_solo_human`, `is_solo_youkai`, `team()` new on `ShotType`), `sht::team` calls in pshot → `ShotType::team`,
`GaugeLimits::for_shot(ShotType)`. (2) `types::angle_from` (Player_AngleFrom 08 §3.1, body of bullet.rs:81; bullet.rs
switches in 11h); `Player::angle_from` (unused) removed; `Angle` for `Opt.angle` / `Opt.facing`, `PBullet.angle`,
`Player.option_angle` (`.wrap_add` for the 14 §3 orbit steps). (3) `place_option` split into `yukari_calc` /
`remilia_calc` / `yuyuko_calc` / `youmu_calc` + `OptEnv` snapshot (statement order kept); casts → `From` /
`cast_signed` / `cast_unsigned`, `types::lo8` for the alpha byte stores, `uidx` / `laser_slot` helpers in pshot;
literal separators; `if_not_else` in `move_player` swapped (same branches). `#[expect]` with reason: `Player`
`struct_excessive_bools` (separate original bytes), `damage_against` `too_many_arguments` (callers in enemy.rs, 11e).
Public API: `Player::angle_from` removed; `Player::shot()` new; `GaugeLimits::for_shot` takes `ShotType`;
`Opt.angle` / `Opt.facing` / `PBullet.angle` / `Player.option_angle` are `Angle`; new `types::{angle_from, lo8}`,
`ShotType::{is_solo, is_odd, is_solo_human, is_solo_youkai}` (none used outside core). Lint subset
(tools/lint_subset): player.rs 45 → 0, pshot.rs 41 → 0, types.rs 0 → 0; workspace total after 1 742. Check: FAST
arch_check 0 differing (emit-off 0), cargo test -p th08-core 319 passed, spec_refs 0. Deviations: `damage_against`
keeps 8 args under `#[expect]` instead of an env struct (its callers are not 11g files); `Player::shot()` panics on a
raw byte ≥ 12, unreachable since Player_Load fails on the missing sht / anm files first.
**11b done.** Files: crates/th08-formats/src/reader.rs (new: `Reader` cursor `u8/u16/i16/u32/i32/f32/vec3/u16s/u32s/
array/bytes`, `at` / `seek` / `skip`, `Truncated { at }`; crate-private `u16_at` / `u32_at` / `put_u32` for fixed-size
in-memory records), lib.rs (`pub mod reader`), stdfile.rs (parse on `Reader`; `std::Op` + `CamSlot` decoder beside
`Record`, 06 §4), msg.rs (`MsgOff(u32)`, `Ins.at: MsgOff`, `Ins::next()`, `entries: Vec<MsgOff>`, `ins(MsgOff)`,
`msg::Op` decoder + `MsgFile::op`, 10 §5.1), sht.rs, cfg.rs, replay.rs, scoredat.rs, thbgm.rs, archive.rs, end.rs
(Reader / shared accessors, casts, lints); boundary only: core msg.rs (8 lines: `ip.idx()`, `MsgOff::from_idx(ip)`,
`ins.next().idx()`, tests `MsgOff(8)`). musiccmt.rs needed nothing (byte scanner, no reads). Public API (outside the
crate): `th08_formats::msg::{Ins.at, MsgFile.entries, MsgFile::ins}` typed `MsgOff`, new `msg::{MsgOff, Op}`,
`MsgFile::op`, `stdfile::{Op, CamSlot}`, `reader::{Reader, Truncated}`; error enums: `StdError::ReadPastEnd(usize)` /
`ShtError::Truncated(usize)` → `Truncated(reader::Truncated)`, new `Truncated` variant in `ArchiveError`,
`ReplayError`, `ScoreError` (no user outside formats); `Record::i/f/v` kept for stage.rs / stdview (11f).
New tests: `reader::reads_and_truncation`; `msg::all_msg_ops_decode` (every instruction decodes, counts = 10 §5.1
column for all 23 ops); `stdfile::all_std_parse` checks `Op::decode` and the 06 §4 count column. Lint subset
(tools/lint_subset): formats 86 → 26 (the 26 are anm.rs 10 / ecl.rs 16, runs 11c / 11d); run files 60 → 0;
workspace 1 768 after. Check: FAST arch_check 0 differing (32/32 identical), cargo test 422 passed, spec_refs 0.
Deviations: malformed-input paths that panicked before now return errors (std negative quad size → `QuadSize`, archive
table running past its end → `BadTable` / `Truncated`); valid files parse identically. Writers (`to_bytes`, `encode`,
`header`, `pack`) keep direct slice stores.
**11c done.** Files: crates/th08-formats/src/anm.rs (`AnmOff(u32)`, header-only `Ins { off, op, size, time, mask }`
with `read` / `nargs` / `raw` / `arg`, `Arg { raw, var }`, `signature` (moved from anmvm), `Op` = `Flow(FlowOp)` /
`Sprite(SpriteOp)` / `Interp(InterpOp)` / `Var(VarOp)` / `Unknown(i16)` with `Cmp`, `Arith`, `MathFn`,
`Op::decode(&Ins, bytes)`; THTX `as_chunks`, `cast_signed`, test type alias; tests: decode ⇔ signature for every
i16, every vanilla instruction decodes (17 117), truncated-arg read), crates/th08-core/src/anmvm.rs (`VmFlags`
bitflags, the `F_*` consts now `VmFlags::X.bits()` (same values, test); `run` = loop + `exec_op` + `ops_flow` /
`ops_sprite` / `ops_interp` / `ops_var`, decode lazily after the time test; `Step::{Jump(AnmOff), Interrupt}`;
`var()` id table + `Dest::Lit`; `lo8` / `lo16` / `table_index` quirk helpers; `compare` / `int_arith` /
`float_arith`; `D3dColor` for init colour and `interp_color_from`), crates/th08-core/src/anmmgr.rs (flag tests via
`VmFlags`, `Tint` via `D3dColor`, `lo16_index`, `From` casts). Public API: `AnmVm.ip` / `saved_ip`:
`Option<AnmOff>` (was `Option<usize>`; outside users only call `.is_some()`); `anmvm::signature` →
`th08_formats::anm::signature`; new `anmvm::VmFlags`, `AnmVm::{vm_flags, has, insert, remove}`, `th08_formats::anm::{AnmOff, Ins,
Arg, Op, …}`; `anmvm::disasm` unchanged. Lint subset (tools/lint_subset): run files 84 → 0 (anmvm 62, anmmgr 12,
formats/anm 10); workspace 1 744 (11a reported 1 823). Check: FAST arch_check 0 differing (424 tests), core 318 + formats
28 tests pass, spec_refs 0; `session golden --no-emit --no-drawlog` 2.30–2.53 s vs 2.35–2.56 s for master's binary (risk 2: no
slowdown). Deviations: the pub fields `flags: u32` and `color` / `color2` / `color_start` / `color_end` /
`color2_start` / `color2_end: [u8; 4]` keep their raw types. They are written in ~20 other files, among them eclvm
(11d), player and pshot (11g), which run in parallel. So `VmFlags` / `D3dColor` are used inside anmvm / anmmgr, and
callers keep `F_*` / `[A]`. Switching the field types is left to the owners' runs, or to one small run after wave 1
(the new accessors give each owner the typed form). Jump targets are `AnmOff(ival.cast_unsigned())`, where the old
code used `ival as usize`. The two differ only for negative offsets, and both fail the instruction read and end the
script.
**11d done.** Files: crates/th08-formats/src/ecl.rs (`EclOff(u32)` for `Ins.off` / `TlIns.off` with `idx()` / `rel()`;
`Arg { raw, var }`, `Dst { slot, arg }`; `Op` = `Flow` / `Var` / `Move` / `Enemy` / `Bullet` / `Laser` / `Boss` group
enums with named fields, `Unknown(i16)`; `Op::decode(&Ins, &[u8])` (one opcode switch → the group's switch) and
`Ecl::op`; `BulletBlock` / `LaserBlock` / `SpellBlock` parsers (07 §9.1, §9.4, §11); `signature` arms merged; new test
`decode_every_table_op`, the vanilla-file test now asserts every executed op decodes), crates/th08-core/src/eclvm.rs
(run loop, argument readers on `Arg` / `Dst`, `exec` = `match Op` → group fn, `at` / `at_mut` / `lo8` / `lo16`
helpers; tests unchanged except imports), new crates/th08-core/src/eclvm/{vars, ops_flow, ops_var, ops_move,
ops_enemy, ops_bullet, ops_laser, ops_spell, ops_native}.rs (`IRef` / `FRef` → `IntVar` / `FltVar::from_id`; named
consts `BULLET_MARKED` 0x100000, `BLK_YOUKAI_ONLY` / `BLK_HUMAN_ONLY`, `DESC_SFX`, `WHITE`), crates/th08-tools/src/bin/
eclview.rs (`-o` prints the decoded `Op`). Public API: `th08_formats::ecl::Ins.off` / `TlIns.off` are `EclOff` (was
`usize`); new `ecl::{EclOff, Arg, Dst, Arith, Cmp, SpawnKind, Op, FlowOp, VarOp, MoveOp, EnemyOp, BulletOp, LaserOp,
BossOp, BulletBlock, LaserBlock, SpellBlock}`, `Ecl::op`; `Ecl::ins` / `tl_ins` / `sub_start` signatures unchanged
(core `Context.ip` stays `usize`). Lint subset (tools/lint_subset) for the run's files: before 154 (eclvm 137,
ecl.rs 16, eclview 1), after 0; workspace 1 674 in 67 files. Check: FAST arch_check 0 differing, cargo test 421
passed (+1 formats test), spec_refs 0, SPEC-GAP unchanged. Perf (§4 risk 2): user instructions on fullrun_noclip
130.449 G → 130.515 G (+0.05 %); ECL symbols 0.7 % → 0.8 % of cycles. Wall time under parallel agent load was
+5 % ± noise (cycles of unrelated fns such as `graze_test` varied 3×), so re-time on an idle machine if wanted.
Deviations: two op files beyond the plan's six (`ops_var` for ops 6–39 / 86 / 87 / 166 / 169, `ops_enemy` for
ANM / flags / spawns / items / effects) plus `vars.rs`; group enums nested under `Op` (one flat 180-variant `match` would exceed the 150-line limit); block ops
(`Bullets`, `Laser`, `Spellcard`) carry no fields: the block is parsed from the instruction bytes because
`Enemy.stored_fire` (enemy.rs, 11e) stores the raw bytes; enemy flag literals (`1 << 25`, `1 << 30`, flags B `1 << 1`,
`1 << 8`) stay raw for 11e's `EnemyFlags`.

**11i done.** Files: crates/th08-core/src/bomb.rs, crates/th08-core/src/effect.rs. (1) bomb: every colour literal is a
named `D3dColor` const with its 14 § row (`BLAST_COLOURS`, `DOLL_FLASH` / `DOLL_OCTAGON` / `DOLL_LS_CORNERS`,
`START_FX`, `ALICE_FLASH`, `KNIFE_HIT`, `KNIFE_LS_FLASH`, `YOUMU_FX` / `YOUMU_LS_FX` / `YOUMU_FLASH`,
`YUYUKO_LS_FLASH`, `RGB_WHITE` / `RGB_RED`, the 14 §1.3 tints `TINT*` + `TINT_ALPHA`, `YUYUKO_GROUPS`); `tint`,
`flash`, `playfield_rect` take `D3dColor` (channel bytes via `r() g() b()`, `lo8` for the masked ramp / alpha);
local `spawn` helper (Effect_Spawn through `FxEnv`); `Hit { period, max, no_spark }` replaces the 9-arg `dc`,
`SparkRing` the 8-arg `spark_ring`; `ObjState { Free, Active, Exploding }` (`repr(i32)`, 14 §1.1) for `BombObj.state`;
`Bomb.fx: Option<EffectIdx>`; `obj_int` (`usize` < 128 → loop int, `#[expect]`), `anmvm::table_index` for int → index.
(2) effect: `FxEnv { kind, pos, param, colour: D3dColor, zoff }` (13 §2.2) for `init_start` / `spawn_at` /
`spawn_fixed_param` (+ `reset`); `EffectIdx` inside the manager (`Index<EffectIdx>` on `Effects`, draw lists,
`spawn_ring` / `slot_high` / `init_start` / `type_init` / `update` / draws, `BgDraw`), `slot` / `raw` boundary
helpers; named `WHITE`, `DEATH_RING`, `INVULN_RING`, `ORBIT_SPIN` (0.031_415_9), `YUKARI_FLASH` / `YUKARI_LS_FLASH`,
`SIDE_SCALE` (f32 0x3F3504F2); `ftol_u8` (`#[expect]`, ftol byte store); `f64::from` / `cast_signed` /
`cast_unsigned` casts; `alloc_screen` `unwrap_or_else`; collapsible / semicolon / `is_multiple_of` fixes. State enum:
`ObjState` (the only mode-like state word); `Effect`'s four bools stay (separate original bytes +0x350 / +0x352 /
+0x355 / +0x357, written by enemy / eclvm) under `#[expect(struct_excessive_bools)]`; `Effect.layer` stays `u8`
(stage.rs writes it). Public API: `Effects::spawn_at(anm, rng, FxEnv, count) -> Option<EffectIdx>` and
`spawn_fixed_param(anm, rng, FxEnv, k) -> EffectIdx` (only callers: bomb, effect tests); `write_pos_unchecked`
takes `EffectIdx` (only bomb); `BgDraw::{Flat, Billboard.i}` hold `EffectIdx` (stage.rs only stores / passes them
back); new `effect::{FxEnv, WHITE}`, `bomb::ObjState`; `BombObj.state: ObjState`, `Bomb.fx: Option<EffectIdx>` (no
user outside bomb). Unchanged raw forms for the other runs' files: `spawn` / `spawn_param` / `spawn_fixed` /
`spawn_high` (`u32` colour, `usize` / `NONE` result), `get` / `peek` / `free` (`usize`), `pub fx: Vec<Effect>`.
Lint subset (tools/lint_subset): bomb.rs 104 → 0, effect.rs 65 → 0; workspace 1 444 → 1 275 (53 files; other
wave-2 runs not merged). Check: FAST arch_check 0 differing (emit-off 0), cargo test -p th08-core 323 passed (1 ignored),
spec_refs 0. Deviations: `spawn_param` keeps its 8 args under `#[expect(too_many_arguments)]` (caller
eclvm/ops_enemy.rs, 11e); no `BombEnv`: the bomb's per-frame context is the existing `Bf`, the arg bundles are `Hit`
/ `SparkRing`; effect / SE / script ids stay short hex (they are the spec's numbers).

Order: 11a → wave 1 (11b ∥ 11c ∥ 11d ∥ 11g) → wave 2 (11e ∥ 11f ∥ 11h ∥ 11i ∥ 11j) → 11k → wave 3 (11l ∥ 11m ∥ 11n)
→ 11o → 11p. 16 runs (≈ A's estimate of 15). L2 (launcher Design 2 run 2, settings.rs / launcher.rs) runs in
parallel with any core run and must land before 11o deliverable 3.

## 3. Rules for every run

1. **Frozen semantics.** No change to arithmetic, operand order of float expressions (`a * b + c` stays, no
   reassociation, no `mul_add`, no `f32::midpoint` for `manual_midpoint`), integer widths of intermediate values,
   call order, RNG draw order and draw-command order. `match` arm order may change only where arms are disjoint
   literals; guards keep their order. `match_same_arms` merges only arms with identical bodies.
2. **Casts.** `cast_lossless` → `T::from` / `.into()`; `cast_possible_wrap` / `cast_sign_loss` on integer
   reinterpretation → `cast_signed()` / `cast_unsigned()` (same bits); a narrowing that is the original's low-byte /
   low-word store → a named helper (`lo8`, `lo16`) or `as` with `#[expect(clippy::…, reason = "spec §")]`. Float→int
   and int→float stay `as` (ftol / fild; saturation on overflow is what the port does today — not changed here).
3. **Quirks in named helpers.** Each odd clamp, off-by-one, `>=` vs `>` angle wrap, double-draw of RNG, original
   bug kept for bit-exactness becomes (or stays) a small named fn or const with its spec citation and 1–3 lines of
   why (CLAUDE.md). Splitting a long fn keeps the statement sequence; extracted fns take `&mut` to exactly what the
   original touched.
4. **Types.** Newtypes are `#[repr(transparent)]` / `#[repr(i32|u8)]`, with `.0` / `as i32` at save, replay, trace,
   draw-log and ECL-variable boundaries (raw values stay in formats and traces). Flag words keep their bit values
   (`bitflags` with the spec's bit positions; `from_bits_retain` for words loaded from files or scripts, never
   `from_bits_truncate`). Typed indices wrap the same slot number; pools keep the original order and free-slot search.
5. **VM `Op` enums (D6).** One style for ANM / ECL / STD / MSG: formats keeps the byte-offset `Ins` and provides a
   cheap `Op::decode(&Ins, bytes)`; core decodes lazily per execution (ECL bytes stay writable, 9c). Unknown or
   unused opcodes → `Op::Unknown(op)` with the spec's behaviour (ignored, as today). Each arm or op fn cites its spec
   row. Argument reads that consume RNG take `&mut Rng` in the signature.
6. **No behaviour or format changes.** Traces, draw logs, score.dat / replay / cfg bytes identical (11p: draw log only,
   visually identical). A difference in T or D = revert the step, not adjust the reference. A divergence the spec does
   not explain → `// SPEC-GAP:` + question (CLAUDE.md), never trace-fitting.
7. **Public API.** Each run report lists every changed `pub` item used outside its crate (app, tools, bins:
   session / demosync / drawrun / eclview / anmview / stdview, arch_check), e.g. 11k `Game.scene: SceneId`, 11d / 11c
   `Ins.off: EclOff` / `AnmOff`, 11a shim paths, 11h `BulletIdx` in trace readers, 11p `ResCmd::CreateImage`.
8. **Comments.** Spec-citation comments move with the code they describe; none are dropped. New modules get the
   one-line `//!` naming what they port and the spec section.
9. **Tests.** Existing tests keep their assertions; new unit tests only for new helpers (decoders, `Reader`,
   flag round trips `from_bits_retain(x).bits() == x`, `Op::decode` of every opcode in the vanilla files).

## 4. Risks

1. **`as` → helpers changes semantics** if a reinterpretation is replaced by a checked or saturating conversion
   (`try_from`, `clamp`). Rule 2 forbids; reviewers grep the diff for `try_from` / `saturating_` / `clamp` in logic.
2. **Decode cost**: lazy `Op::decode` per executed instruction on top of the header read; ECL / ANM run thousands of
   instructions per frame — measure `session golden` wall time before / after 11c / 11d (≤ 5 % allowed).
3. **Large-file conflicts**: eclvm (3 619 lines) and enemy (3 676) are split over 11d / 11e and touched by no other
   run except the index boundary; worldclone.rs only in 11k / 11o.
4. **11p D refresh** hides later draw bugs if done early: it runs last, alone, with my image diff before the freeze.
5. **Device-loss rebuild (11n)** is untestable on desktop by T / D; manual check by forcing `rebuild()` from a debug
   key, plus Android emulator suspend / resume (CLAUDE.md: emulator only).
