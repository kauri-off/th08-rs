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
| **11e** enemy — **DONE** (see below) | (1) `EnemyIdx` for chain / boss / thread links; (2) `EnemyFlags` for the 07 flag words, `TlOp` timeline enum (enemy.rs:1139); (3) long fns 1133 / 1392 split, enemy subset 0 | enemy, eclvm (index boundary only) | after 11d; ∥ 11f 11h 11i | T D |
| **11f** STD + MSG | (1) stage.rs on `std::Op`, `Stage` bools; (2) msg.rs on `msg::Op`, exec split, `Dialogue` bools; (3) both subset 0 | stage, msg | after 11b; ∥ 11e 11g 11h | T D |
| **11e** enemy | (1) `EnemyIdx` for chain / boss / thread links; (2) `EnemyFlags` for the 07 flag words, `TlOp` timeline enum (enemy.rs:1139); (3) long fns 1133 / 1392 split, enemy subset 0 | enemy, eclvm (index boundary only) | after 11d; ∥ 11f 11h 11i | T D |
| **11f** STD + MSG — **DONE** (see below) | (1) stage.rs on `std::Op`, `Stage` bools; (2) msg.rs on `msg::Op`, exec split, `Dialogue` bools; (3) both subset 0 | stage, msg | after 11b; ∥ 11e 11g 11h | T D |
| **11g** player + pshot — **DONE** (see below) | (1) `ShotType` enum (+ `sht::team`) inside player / pshot; (2) `Angle`, one `angle_from` (player.rs:668 → types); (3) player / pshot subset 0 (pshot 260 split) | player, pshot | ∥ wave 1 | T D |
| **11h** bullet + item | (1) `BulletFlags` / `LaserFlags`; (2) `BulletIdx` / `LaserIdx` / `ItemIdx`, `angle_from` (bullet.rs:81) → types; (3) spawn env struct, fn 657 split, subset 0 | bullet, item | after 11g (`angle_from`); ∥ 11e 11f 11i | T D, **B** |
| **11i** bomb + effect — **DONE** (see below) | (1) bomb named consts + `D3dColor` (75 literals); (2) effect `FxEnv` (4 fns), `EffectIdx`, state enum; (3) both subset 0 | bomb, effect | ∥ 11e 11f 11h | T D |
| **11h** bullet + item — **DONE** (see below) | (1) `BulletFlags` / `LaserFlags`; (2) `BulletIdx` / `LaserIdx` / `ItemIdx`, `angle_from` (bullet.rs:81) → types; (3) spawn env struct, fn 657 split, subset 0 | bullet, item | after 11g (`angle_from`); ∥ 11e 11f 11i | T D, **B** |
| **11i** bomb + effect | (1) bomb named consts + `D3dColor` (75 literals); (2) effect `FxEnv` (4 fns), `EffectIdx`, state enum; (3) both subset 0 | bomb, effect | ∥ 11e 11f 11h | T D |
| **11j** gamemgr + replay | (1) `GameFlags` bitflags + `end()` / `route()`; (2) `Difficulty`, `Paused`, `Loading` enums inside gamemgr / replay / game / world; (3) long fns 713 / 1078 split, flow-core subset 0 (gamemgr, replay, game, world, chain, task, trace, sound, input, rng, host, config, drawfmt) | gamemgr, replay, game, world, chain, task, trace, sound, input, rng, host, config, drawfmt | after 11a; ∥ 11e–11i | T D |
| **11i** bomb + effect | (1) bomb named consts + `D3dColor` (75 literals); (2) effect `FxEnv` (4 fns), `EffectIdx`, state enum; (3) both subset 0 | bomb, effect | ∥ 11e 11f 11h | T D |
| **11j** gamemgr + replay — **DONE** (see below) | (1) `GameFlags` bitflags + `end()` / `route()`; (2) `Difficulty`, `Paused`, `Loading` enums inside gamemgr / replay / game / world; (3) long fns 713 / 1078 split, flow-core subset 0 (gamemgr, replay, game, world, chain, task, trace, sound, input, rng, host, config, drawfmt) | gamemgr, replay, game, world, chain, task, trace, sound, input, rng, host, config, drawfmt | after 11a; ∥ 11e–11i | T D |
| **11k** scenes — **DONE** (see below) | (1) `SceneId` for every `SCENE_*` (123 uses, 15 files, incl. tools drawreport / demosync / drawlog, app 1); (2) result state enum + splits (340, 1614), supervisor `box_default` + split (444); (3) ending / musicroom / supervisor / result subset 0 | gamemgr (consts), supervisor, result, ending, musicroom, ascii, menu, menulist, game, replay, worldclone, tools, src/app.rs (1 line) | **alone** (touches UI + flow + app) | T D, **B** |
| **11l** HUD + GUI — **DONE** (see below) | (1) hud unwraps → `let … else`; (2) named layout consts, splits 339 / 950 / 1297; (3) hud / gui subset 0 | hud, gui | after 11k; ∥ 11m 11n | T D |
| **11m** ascii + menus | (1) one `percent_glyphs` (ascii.rs:912 + menulist.rs:421); (2) `MenuEnv` for ascii 1035 / 1322, splits menu 539, menulist 776 / 1316; (3) ascii / menu / menulist subset 0 | ascii, menu, menulist | after 11k; ∥ 11l 11n | T D |
| **11l** HUD + GUI | (1) hud unwraps → `let … else`; (2) named layout consts, splits 339 / 950 / 1297; (3) hud / gui subset 0 | hud, gui | after 11k; ∥ 11m 11n | T D |
| **11m** ascii + menus — **DONE** (see below) | (1) one `percent_glyphs` (ascii.rs:912 + menulist.rs:421); (2) `MenuEnv` for ascii 1035 / 1322, splits menu 539, menulist 776 / 1316; (3) ascii / menu / menulist subset 0 | ascii, menu, menulist | after 11k; ∥ 11l 11n | T D |
| **11n** render | (1) gfx.rs executor split (264); (2) device-loss rebuild from the provenance table; (3) render subset 0 (+ text.rs) | crates/th08-render | **after the current render / audio / app work lands**; ∥ 11l 11m | T D; manual desktop + Android emulator suspend / resume |
| **11n** render — **DONE** (see below) | (1) gfx.rs executor split (264); (2) device-loss rebuild from the provenance table; (3) render subset 0 (+ text.rs) | crates/th08-render | **after the current render / audio / app work lands**; ∥ 11l 11m | T D; manual desktop + Android emulator suspend / resume |
| **11o** audio + app + tools + wall — **DONE** (see below) | (1) midi.rs wasm imports, audio subset 0; (2) tools subset 0 (useless_conversion ×10 etc., bin `main` splits), worldclone / testdata subset 0; (3) app subset 0 after L2; arch_check gains `cargo clippy --workspace --all-targets -- -D warnings` (subset at `warn` → enforced) | crates/th08-audio, crates/th08-tools, worldclone, testdata, src/ (not settings.rs until L2), tools/arch_check | last D-neutral run; after L2 and 11n | T D W android, **B** |
| **11p** leftovers that change draw — **(1)–(2) DONE** (see below; (3) orchestrator) | (1) camera.rs + menulist pie on `fmath` (drop both `allow(disallowed_methods)`, the `TODO(step 10)`); (2) `image` out of core: `th08_formats::img::dims` + `ResCmd::CreateImage`, decode in gfx; (3) new D reference (`arch_check freeze` into `target/arch_ref_11p`) after my drawgroups image diff shows "visually identical" (PARITY_POLICY) | camera, menulist (2 lines), drawlist, formats/img.rs (new), gfx (CreateImage), anmmgr (load call) | **alone**, after 11o | T must be identical; D intentionally new; drawgroups image diff; B |

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
**11j done.** Files: crates/th08-core/src/{gamemgr, replay, game, world, chain, trace, sound, input, rng, config,
drawfmt}.rs (task.rs, host.rs had no subset warnings and are unchanged). (1) gamemgr: `GameFlags` bitflags (11 §2.1 bit
positions, `const _ = !0` keeps unnamed bits; `END` / `ROUTE` / `UNLOCKS` masks, `end()` / `with_end()` / `route()` /
`with_route()`); the `F_*: u32` consts stay as `GameFlags::X.bits()` aliases; `STAGE_START_CLEARS` names `~0x2180`
(11 §3.1); new `Game::{game_flags, set_game_flags, has, insert_flags, remove_flags, pause_state, set_paused,
loading_state, set_loading}`, `diff()` now `pub` and returns `Difficulty` (was private `usize`); `end_state` /
`route` / `practice` / `replay` … delegate to `GameFlags`. (2) `Paused { No, Open, Drawn }`, `Loading { Done, Active,
Failed }` (`#[repr(u8)]`, gamemgr) used in gamemgr; `Difficulty` in gamemgr / replay; replay `fast_forward` and
`new_recording` take `GameFlags`, `start_playback` keeps `UNLOCKS | REPLAY`. (3) `stage_start` → `begin_stage` (to
Hud_Load's fresh flag) + `player_stats` + free `gauge_lead` / `spell_background_base` / `load_stage_bgm`; `calc` →
`loading_step` + `pause_key` + `demo_step`; statement order unchanged. Lint fixes: `From` / `cast_signed` /
`cast_unsigned` / `lo8`, replay-local `lo32` (i64 score → u32 low dword), `is_multiple_of`, range `contains`, typed
`::default()`, `#[expect]` with reason for ftol float→int (`record_stage_start`, sound MIDI fade), the 128-pan average,
trace `col(usize)` and `Game`'s `struct_excessive_bools` (independent GM bytes). Test added: `game_flags_bits`
(round trip, 2-bit fields, `0x2180`, paused / loading bytes, `diff` clamp). Public API: `gamemgr::{GameFlags, Paused,
Loading}` + the `Game` methods above, `Game::diff` public with a new return type; `Game.{flags, difficulty, paused,
loading}` fields and every `F_*` / `SCENE_*` const unchanged, so no out-of-crate caller changes. Lint subset for the
run's files: before 166 (replay 46, gamemgr 43, trace 29, drawfmt 11, world 9, game 8, rng 8, sound 7, chain 2,
input 2, config 1), after 0; workspace 1 278 in 44 files at the time of the run (parallel runs included). Deviations:
the `Game` fields stay raw (`flags: u32`, `difficulty: i32`, `paused` / `loading: u8`) because ascii, menu, menulist,
hud, effect, enemy, eclvm, supervisor, result, player, worldclone, src/platform/touch.rs and tools read or write them
(≈ 120 sites in files of other runs); `Sys.flags` stays `&mut u32` (a `&mut GameFlags` needs the field typed). The
field switch fits 11k, which touches most of those files. `game_over` (0 / 1) kept as `u8` (not in the row). Check: FAST
arch_check 0 differing, cargo test -p th08-core 324 passed (+1 test), spec_refs 0, SPEC-GAP unchanged.
**11e done.** Files: crates/th08-core/src/enemy.rs; eclvm boundary only: eclvm.rs (tests: link / boss-table literals
`Some(EnemyIdx(n))`, one `spawn` call → `Spawn`), eclvm/ops_enemy.rs (child-link tail `EnemyIdx`), ops_native.rs (2
`i.idx() == self.idx` compares), ops_spell.rs (boss table / `Spell.boss` / `spell_boss` writes via
`EnemyIdx::from_slot`). (1) `Enemy.chain_prev` / `chain_next` / `root`, `EnemyManager.bosses`, `Globals.spell_boss`,
`Spell.boss` are `EnemyIdx`; `EnemyIdx::from_slot(usize)` (inherent impl in enemy.rs, types.rs untouched) and
`Index` / `IndexMut<EnemyIdx>` for the `Vec<Box<Enemy>>` pool, so chain walks index unchanged. (2) `EnemyFlags` /
`EnemyFlagsB` bitflags with the 07 §4.4 bit positions (incl. the formerly literal bits A 5 `INVISIBLE`, 25 `ROTATE`, 31
`BOMB_IMMUNE`, B 1 `SPARKLE`, 8 `SHARED_OFFSET`; multi-bit `MOVE_MODE` / `EASE` / `DEATH_MODE` / `WARN`); the `A_*` /
`B_*` consts are now `EnemyFlags::X.bits()` (same values, test `flag_bits_round_trip`); `Enemy::{a_flags, b_flags,
has, has_b, set, insert, remove, insert_b, remove_b, death_mode, warning}`; every flag test in enemy.rs typed.
`TlOp` (07 §3 table) decoded from the 7 raw args (`TlOp::decode`, test `timeline_op_decode`), `timeline_op` = one
`match TlOp`; new `Spawn` request struct (07 §4.1) for `spawn`. (3) `kill_chain` split per 07 §10.3a step:
`detach_children` (step 2), `reward_children` (step 3), `boss_chain_reward` (step 5), helpers `chain_popup_colour`
(step 1), `child_orbs`; casts → `From` / `cast_signed` / `cast_unsigned` / `lo8` / `anmvm::table_index`, `TRAIL_MAX`,
`if_not_else` in `screen_check` swapped, literal separators. `#[expect]` with reason: `spawn_finish`
`too_many_arguments` (ECL callers pass fields separately), the one ftol `f32 as u32` in `spell_step`,
`struct_excessive_bools` on `Enemy` (port-only per-frame bools), `Spell` / `Globals` (spell-flag bits split across
both, written by eclvm). Public API: `EnemyManager::spawn(anm, Spawn, sys)` (was 8 positional args; only caller
outside enemy.rs: an eclvm test); `Enemy.{chain_prev, chain_next, root}`, `EnemyManager.bosses`,
`Globals.spell_boss`: `Option<EnemyIdx>`, `Spell.boss: EnemyIdx`; new `enemy::{EnemyFlags, EnemyFlagsB, Spawn}`;
`EnemyManager::boss(slot)` still returns `Option<usize>` (hud / eclvm compare it with usize slots); no user in
app / tools changed (session.rs reads `flags` / `flags_b` / `A_BOSS`, unchanged). Lint subset (tools/lint_subset):
enemy.rs 85 → 0, eclvm files 0 → 0; workspace 1 359 in 54 files after (parallel runs' worktree base). Check: FAST
arch_check 0 differing (emit-off 0), cargo test -p th08-core 325 passed (+2) / formats 36, spec_refs 0, SPEC-GAP 1. Deviations:
the `flags` / `flags_b: u32` fields stay raw (eclvm writes them in ~40 places; 11c's `VmFlags` pattern), so eclvm keeps
`A_*`; `running_thread` (a thread slot 0–3, not an enemy) and `draw_lists` / `idx: usize` parameters stay `usize`;
`drop_items` power kind written `i32::from(power >= 128.0)` (differs from `power < 128 ? 0 : 1` only for NaN power,
which no path produces); the old 1133 / 1392 fns (105 / 122 lines) were under the 150 limit, both split anyway
(`timeline_op` now 70 lines).
**11f done.** Files: crates/th08-core/src/stage.rs, crates/th08-core/src/msg.rs (formats untouched: the 11b
decoders `stdfile::Op` / `msg::Op` were complete). (1) `Stage::exec` = `match stdfile::Op::decode(&r)`, one arm per
06 §4 op with its citation (`CamSlot::index()` replaces the `(op − 5) / 2` / `(op − 14) / 5` arithmetic); the op-3
clock test in `step` and the op-31 search in `handle_section_request` compare decoded `Op::Halt` / `Op::Section(req)`;
op 33's byte store is `types::lo8`; casts → `From` / `cast_signed` / `cast_unsigned`, the two ftol colour bytes (06 §8
fog lerp, 06 §11.1 soft-fog alpha) keep `as u8` under `#[expect(cast_sign_loss)]`; literal separators,
`is_multiple_of`, collapsed sparkle `if`, `if_not_else`, test `AnmVm { sprite_w, .. }`. (2) `Dialogue::exec` =
`match msg::Op` → `exec_portrait` (ops 1 2 5 8 15 17), `exec_text` (3 13 16 18 19 20, `text_on_line` for 19 / 20),
`exec_flow` (0 4 6 7 9–12 14 21 22, unknown), statement order per op unchanged; `Dialogue.ip: MsgOff` (was `usize`,
the `from_idx` / `idx()` round trips gone); i16 index args through `arg_index` (`usize::from(v.cast_unsigned())`:
negatives fail every `< 4` test and clamp under `min` exactly as the old sign-extending `as usize`); op 15 / 17
speaker byte `lo8`; `argb` = `D3dColor(c).to_bgra()` (same bytes, B G R A = 0 1 2 3); `face_files` stage-2 arm merged
into the identical wildcard. (3) `Stage` and `Dialogue` bools under `#[expect(struct_excessive_bools)]` with reason:
each is a separate original field / byte (g_Stage +0x6464, +0x625C, +0x647C, HUD cover; HudImpl +0x22D7C.., +0x22D84,
port-only `side_effects`), not one flag word (rule 4). Public API: none changed (`msg::argb` same signature; `ip` is
private). Lint subset (tools/lint_subset): stage.rs 43 → 0, msg.rs 27 → 0; workspace 1 374 in 53 files (worktree base 9cada7f). Check: FAST arch_check 0
differing (emit-off 0), cargo test -p th08-core 323 passed + th08-formats 36 passed, spec_refs 0. Deviations: the bools stay (with `#[expect]`) instead of bitflags
or enums, as in 11g's `Player`; `Stage.ring_6b: Option<usize>` stays raw (an effect slot, `EffectIdx` is 11i's);
`Record::i/f/v` are no longer used by stage.rs (stdview and `Op::decode` still use them).
**11h done.** Files: crates/th08-core/src/bullet.rs, item.rs; index / import boundary in eclvm/ops_laser.rs (3
lines), eclvm/ops_move.rs and eclvm/vars.rs (import line only). (1) `bullet::BulletFlags` (u32, every ex-command
bit of 08 §5.1 + spawn effects 2/4/8, pattern sfx 0x200, no-cancel 0x1000; original values) for `Bullet.active` and,
through `Bullet::bflags()` / `cancellable()`, for `Bullet.flags`; `ExCmd.cmd` matched as `BulletFlags` consts;
`LaserFlags` (u16: `ALPHA_FADE` bit 0, `NO_CANCEL` bit 2) for `Laser.flags`; all `from_bits_retain`. (2) draw lists
`Vec<BulletIdx>`, `place` / `apply_ex` / `update_*` / `push` on `BulletIdx`; `spawn_laser -> Option<LaserIdx>`,
`laser_mut(LaserIdx)`, `update_laser(LaserIdx)`; item `order: Vec<ItemIdx>` (tombstone `ItemIdx(u16::MAX)`),
`ItemManager::spawn -> Option<ItemIdx>`, `collect` / `convert_power_items` on `ItemIdx`; `bullet::angle_from` removed,
all callers (item, eclvm ops_laser / ops_move / vars) on `types::angle_from`; item auto-collect test on
`Player::shot()` (`MarisaAlice | Marisa`). (3) `PatternEnv { d, aim, ppos }` → `place` 6 args; `apply_ex` = slot loop
+ `start_ex` ("→ return" commands) + `retype` (0x4000) + `sub_pattern` (0x1000000), statement order kept;
`free_slot` split from `place`; helpers `uidx`, `at` (signed table lookup, negative misses as before), `template_idx`,
`bidx` / `iidx`, `types::lo8` for byte stores (item type, alphas, cancel item mode); literal separators; `ex_sound`
moved above the tests. `#[expect]` with reason: template height ftol (`cast_sign_loss`), warp `v * -1.0`
(`neg_multiply`, kept as the original's fmul). Public API: `Bullet.active: BulletFlags` (was `u32`; demosync prints it
with `{:x}`, unchanged output), `Laser.flags: LaserFlags`, `BulletManager::{spawn_laser, laser_mut}` typed
`LaserIdx`, `ItemManager::spawn -> Option<ItemIdx>` (all outside callers discard it), `bullet::angle_from` gone, new
`bullet::{BulletFlags, LaserFlags}`; `Bullet.flags`, `BulletDesc.flags`, `LaserDesc.flags`, `ExCmd.cmd` stay raw
`u32` (written by eclvm ops_bullet / ops_native and an eclvm test); `bullet::F_NO_CANCEL` kept as
`BulletFlags::NO_CANCEL.bits()`. Lint subset (tools/lint_subset): bullet.rs 76 → 0, item.rs 22 → 0 (eclvm files
0 → 0); workspace 1 346 in 53 files after. Check: FAST arch_check 0 differing (emit-off 0), cargo test -p th08-core 323 passed, spec_refs 0. Deviations: enemy `lasers: [Option<usize>; 32]`
(enemy.rs, 11e) stays raw, converted in ops_laser (`usize::from` / `LaserIdx(id as u16)`); `Bullet.angle` not `Angle`
(not in the row; eclvm tests and worldclone write it).

**11k done.** Files: crates/th08-core/src/{gamemgr, supervisor, result, ending, musicroom, ascii, menu, menulist,
game, replay, worldclone}.rs, crates/th08-tools/src/{drawreport, demosync, drawlog, session}.rs, src/app.rs (2
lines: the two `SCENE_GAME` compares), and boundary lines outside the row (forced by the field type): player.rs
(`matches!(requested, 3 | 11 | 12)` → `NextStage | Reload | Advance` + import), trace.rs:62 (`i64::from(scene as
i32)`), src/platform/touch.rs (2 lines). (1) `Game.scene` / `requested` / `previous: SceneId`, `Game::request(SceneId)`,
`next_scene_after: Option<SceneId>` (the original's −1 = `None`; `>= 0` → `if let Some`, `< 0` → `is_none()`); the
`SCENE_*` consts removed, every use `SceneId::X`; the two `request(-1)` (menu quit, menulist bad replay) →
`SceneId::Quit`, the supervisor's `req < 0` → `req == SceneId::Quit`; `as i32` at the draw-log (`game.rs` →
`drawfmt`), trace, session and log-line boundaries (log text unchanged: numbers). (2) result: `State` `#[repr(i32)]`
enum (16 §2.1 / §2.4 / §2.6 values, incl. `ScoreLoad` 0x12 for the existing exit test, `is_save()` / `is_play()` for
the 10–14 / 0x13–0x15 ranges), `trace_result` gets `state as i32`; `calc_state` = dispatch + `leave` / `diff_page` /
`char_page` / `group_page` / `card_char_page` / `panel` / `panel_out` / `save_msg` (bodies unchanged); `score_txt` →
`txt_card_names` / `txt_entry` / `txt_cards` + free `txt_sj` / `txt_line` / `hscr_i32` / `cstr` (were closures),
output order unchanged (score_txt_reference passes). supervisor: `Box::default()` for Ending / MainMenu (both
`Default` = `new()`), `switch` → `open_menu` (shared x → 1 / 2 → 7 tail) + `retry` (2 → 10). (3) lint fixes:
`table_index` for signed indices, `From` / `cast_signed` / `cast_unsigned`, `lo8` (play-data alpha bytes), local
`lo16` (pending interrupt word), `score_u32` (0..=999 999 999 clamp, 15 §2.3) and `int` (usize counts → i32) helpers
with `#[expect]`, `vm_argb` = `D3dColor::from_bgra(color).0` (result + musicroom, same bits), ftol `as u32` under
`#[expect(cast_sign_loss)]` (ending / musicroom text row, ending picture offset), `bool_to_int_with_if` → `i32::from`
in the else branch (test order kept), `if_not_else` swaps, literal separators, typed `::default()` in tests,
`ResultScreen` `struct_excessive_bools` `#[expect]` (separate fields +0x38 / +0x54 + 2 port flags). Public API:
`th08_core::gamemgr::SCENE_*` removed (use `th08_core::types::SceneId`); `Game.{scene, requested, previous}:
SceneId`, `Game.next_scene_after: Option<SceneId>`, `Game::request(SceneId)` (users in app, touch, tools drawreport /
demosync / drawlog / session updated). Lint subset (tools/lint_subset): run files result 104 → 0, musicroom 32 → 0,
supervisor 23 → 0, ending 14 → 0 (ascii / menu / menulist / tools / app unchanged counts, worldclone 80 → 78);
workspace 865 → 690 in 35 files. Check: FAST arch_check 0 differing (emit-off 0), cargo test -p th08-core 327 passed (1 ignored), th08-tools 24 + 35 passed, th08 app tests pass,
spec_refs 0. Deviations: the other `Game` raw fields (`flags`, `difficulty`, `paused`, `loading`) stay raw (not in
this row; ≈ 120 sites in hud / effect / enemy / eclvm …); boundary edits in player / trace / touch beyond the row's
file list (the field type forces them); app is 2 lines, not 1; supervisor's `switch` was 109 lines (under the limit),
split anyway as planned.

**11l done.** Files: crates/th08-core/src/{hud, gui}.rs. (1) every `.expect` in both files → `let … else` / `if let`
/ `map_or` with a commented fallback: no enemy manager = no boss bar / timer 0 / no card (label cond false, bonus kept),
no player = no clear panel / no panel emit, no dialogue = no lines / events (`load` bails with an error);
`with_hud` / `with_gui` return `Option` (no HUD / GUI = the node does nothing, `Ret::Next` / `Ok(())`). (2) hud: named
layout consts with 12 §5.3 / §5.4 / §5.5 / §5.6 citations (`PANEL_X`, row ys, `STOCK_*`, `TILE_Z_*`, `BAR_*`, `GAUGE_*`,
`TIMER_POS`, `CLEAR_*` pen / step / colours, `POPUP_*`); gui: `STAMP_*`, `DIGIT_SPRITE`, `TEXT_SIZE`, `NAME_STRIP_W`,
name colours, `TINT_START` / `TINT_A_MIN` / `TINT_A_MAX` (same values, same float op order). Splits, statement order
kept: `load` (was 138 lines) → `load_files` / `load_reset` / `load_front` / `load_title`; `draw_clear` (109) → free
`clear_scores` / `clear_final` / `clear_rank` / `clear_clock` + `clear_line` (was a closure); `calc` (124) →
`ecl_requests` / `run_vms` (+ `night_fade`) / `msg_run`. (3) lint fixes: `lo8` (bar alpha, ghost alpha, CATK
difficulty byte), `table_index` for guarded signed indices (BGM slot, rank text, card, face, plate offset),
`From` / `cast_signed` / `cast_unsigned`, u8 tile loop counters (`f32::from` / `i32::from`, exact), `len_i32` with
`#[expect(cast_possible_wrap)]` for popup centring, ftol `as u32` under `#[expect(cast_sign_loss)]` (`line_texture`,
`flush_texts`, `set_boss_plate`), literal separators, method path for `clear_status`. Public API: none outside the
crate changed (`Hud` / `Gui` pub items unchanged; the split fns are private). Lint subset (tools/lint_subset): hud.rs
67 → 0, gui.rs 25 → 0; workspace 598 in 33 files after. Check: FAST arch_check 0 differing (emit-off 0), cargo test -p
th08-core 327 passed (1 ignored), spec_refs 0. Deviations: the fallbacks above replace panics on states the HUD never
sees (no behaviour change in T / D); `night_fade` takes the player position before the alpha test (a pure read,
was read only when alpha ≠ 0); hud keeps its other code hex (sprite ids, ANM slots, VM offsets in comments).
**11m done.** Files: crates/th08-core/src/{ascii, menu, menulist}.rs only. (1) One
`ascii::percent_glyphs(anm, vm, value, at: [f32; 3], colour, each: impl FnMut(&AnmVm))` (12 §5.9
`Ascii_DrawGaugePercent`); menulist's copy removed, `gauge_glyphs` passes its `Option<&mut Recorder>` emit as the
closure, `draw_percent` is the recorder wrapper. The trailing `vm.scale = [1, 1]` of the ascii copy is kept: the
last glyph ('%', full size) already leaves scale 1, so menulist's VM state is unchanged. (2) `ascii::MenuEnv { anm,
rng, g, inp, bgm, sound, hud }`: `PauseMenu::update(e, host)` (was 9 args), `ContinueMenu::update(e, panel, now,
enemies)` (was 11), bodies destructure it so statement order is unchanged; pause states 4 / 9 / 10 at frame 20 →
`PauseMenu::leave` (the `4 | 9 | 10` arm gets the `frame >= 20` guard, below it the arm falls to `_ => {}` as before);
tests call through `cont` / `pause` helpers (assertions unchanged). Splits: menu `top` → `top_entry` (call-0
block), `top_demo` (demorpy load; the idle reset stays in `top`), `top_item` (12 §2.2 item table); menulist
`spell_char_draw_step` → `spell_char_rows` (16 §6.4 steps 2–3) + `spell_char_pies` (steps 5–6), `replay_run` →
`replay_files` / `replay_stages` / `replay_speed` (15 §3.8 sub-states 1 / 2 / 3). (3) helpers in ascii, shared
by the three files: `uidx` (i32 → usize, exactly `as usize`, `#[expect]`), `int` (usize → i32), `short` (usize →
i16 field), `lo16` (pending-interrupt word); `types::lo8` for byte stores (fade alphas, shot / difficulty / speed
bytes, volume); `vm_argb` = `D3dColor::from_bgra(color).0`; `From` for widening and bool → int (incl.
`bool_to_int_with_if` → `i32::from(pressed(DOWN))` in the else branch, test order kept); `is_multiple_of`; literal
separators; ftol `.round() as u32` / sprite-rect `as u32` under `#[expect(cast_sign_loss)]` (`render_strip`,
`render_description`, `capture_into`, `marker_alpha`); `last_difficulty` moved above the test module; typed
`::default()` in tests. Public API: `ascii::draw_percent(anm: &AnmManager, rec, vm, value, at: [f32; 3], colour)`
(was `&mut AnmManager`, `cx, cy, z`), `ascii::percent_glyphs` new signature (no `Option<&mut Recorder>`, takes
`each`), `ascii::draw_now(.., t: &Text)` (was by value); none of the three has a caller outside ascii / menulist.
New `pub(crate)`: `ascii::{MenuEnv, uidx, int, short, lo16}`. Lint subset (tools/lint_subset): ascii 89 → 0, menu 54
→ 0, menulist 98 → 0; workspace 690 → 449 in 32 files. Check: FAST arch_check 0 differing (emit-off 0), cargo test -p
th08-core 327 passed (1 ignored), spec_refs 0, SPEC-GAP 1. Deviations: none of the three old long fns was over the
150-line limit (no `too_many_lines` warning before), split anyway as planned; `PauseMenu::update` keeps its state
`match` (now ≈ 135 lines incl. comments); `replay_speed` returns `bool` like its siblings though it is always true.
**11n done.** Files: crates/th08-render/src/gfx.rs, text.rs, Cargo.toml (native `pollster` dep). (1) The 239-line fn
at gfx.rs:264 was `Gfx::build` (the executor `execute_one` / `execute_res` was already one arm per `DrawCmd` /
`ResCmd`): the device-bound objects move into `struct Gpu` (`new`, `tex_layout`, `draw_pipes`, `blit_pipe`,
`crop_pipe`, `upload`), helpers `shader` / `view` / `encoder` / `extent` / `bind_group` / `write_region` /
`copy_region`, `request_device` shared by `new` / `headless` / `rebuild`. (2) Device loss: per-slot `Provenance`
(size, format, `ResCmd::Create` pixels as the shared `Arc`, None for screen captures) plus ordered `Edit`s (writes
incl. text rasters, copies; an edit inside a later one's rect is dropped), filled by `execute_res`; a device-lost
callback (fresh flag per device) is checked before every submit / `present_last`; `rebuild()` (native, pollster)
drops the old surface before the old device, makes a new surface on the kept window, new adapter / device / `Gpu`,
re-uploads every slot with labels and replays edits; captures come back black. Web: no rebuild (logged), no pixel
copies kept. Debug hook `TH08_DEVICE_LOSS=<n>` (n-th Present) / `force_device_loss()`. (3) Lint subset
(tools/lint_subset crates/th08-render/): gfx 61 → 0, text 13 → 0 (74 → 0); float→int `as` kept under
`#[expect(cast_sign_loss)]` with reasons, `FRAME` u32 const for 640×480. Public API (all additive): `Gfx::rebuild`
(native), `Gfx::force_device_loss`, `Gfx::generation`, `Overlay::device_rebuilt` (default no-op). Check: FAST
arch_check 0 differing (emit-off 0), cargo test -p th08-render 5 passed (new: rebuild round trip bytes + writes +
copy, `rect_within`), wasm32 check clean, spec_refs 0. Manual: desktop `tools/offscreen th08 --no-launcher`
TH08_STAGE=1 TH08_SHOT=900 with TH08_DEVICE_LOSS=400 is pixel-identical to the run without (first try crashed
reconfiguring the old surface onto the new device on Vulkan → surface remade); emulator-5584 (x86_64 debug APK,
bundled data): game in stage 5, two HOME / resume cycles, frame + port menu render, no errors. Deviations: no
separate `Image` provenance (core decodes .jpg / .png into `Create` bytes until 11p); follow-ups outside the row:
app overlays (overlay.rs, portmenu.rs) and launcher.rs keep egui renderers / textures made on `gfx.device` and must
remake them on `Overlay::device_rebuilt` / `Gfx::generation()` change; `TH08_DEVICE_LOSS` is not a settings.rs entry;
wgpu rejects a `ResCmd::Copy` with src == dst (same texture) as a validation error (pre-existing, hud boss plate if
both sprites share a texture).

**11o done.** Files: crates/th08-audio/src/{audio, midi, bgm}.rs; crates/th08-tools/src/{lib, session, demosync,
drawlog, drawreport, xz}.rs + bin/{session, demosync, drawdiff, drawrun, anmview, repack, rsqrt_table}.rs;
crates/th08-core/src/worldclone.rs (testdata.rs had 0); src/{app, platform, pacer, settings, sources, lib}.rs,
src/platform/{touch, safearea}.rs; tools/arch_check; outside the row: core fmath.rs, d3dx.rs, drawlist.rs (see
deviations). (1) midi.rs: `anyhow::{Context, Result}` / `Arc` imports under `cfg(not(wasm32))` like their users (wasm
clippy clean); `RESETS.contains`, `scaled` keeps `as u8` after its 0..=127 clamp under `#[expect]`; audio.rs
`slot_id` (MIDI slot → open-slot i32, 02 §5), `manual_map` / `single_match_else` / `as_chunks` / typed defaults /
struct-update `Config` in tests; device-side float→int `as` under `#[expect(cast_sign_loss)]` with reasons.
(2) tools: `useless_conversion` ×10, `collapsible_if`, `cloned_ref_to_slice_refs`, `type_complexity`
(`session::Column`, `drawlog::Matches`); `th08_tools::int` (usize → i64 trace column, exactly `as`), `lo32` in
session.rs (low word of a trace value / schedule number, exactly `as u32`); bins split: `session` → `parse_args` /
`load_ctx` / `main` / `report_trace` / `report_draw`, `demosync` → `print_rng_calls` / `dump_frame`, `drawdiff` →
`menu_shots` / `demo_shots`, drawlog `diff_frame` → `match_quads` (passes 1–2); statement order kept. worldclone.rs:
trace words via `u64::from` / `i64::from(x).cast_unsigned()` (sign extension = old `as u64`), method paths,
`is_multiple_of`. (3) app: `App` bools grouped into `Focus { focused, pause_audio }` + `holds_audio()` and
`Display { alt, fullscreen }`; `act(&Action)` (Action is launcher.rs, DEVLOSS2); `Settings` / `TouchSource` keep
their bools under `#[expect(struct_excessive_bools)]` (independent table entries / latches, no mode); safearea
`cast_unsigned` / `cast_signed` (Android), `(0i32..).zip` for the JNI corner index; wasm-only sites (`boot_ms`,
`start`, `WebFiles::names`, `get_full_year`) fixed too. Wall: arch_check runs `tools/lint_subset` after the build and
fails on any warning outside `CLIPPY_PENDING` (default: src/launcher.rs, src/platform/overlay.rs,
src/platform/portmenu.rs — DEVLOSS2's files, counted only); `CLIPPY_PENDING=` runs exactly `cargo clippy --workspace
--all-targets -- -D warnings`. Public API (outside the crate): `th08_tools::drawlog::Report::add(key: &str)` (was
`String`), new `th08_tools::int`, `th08_tools::session::Column`; none in core / audio / render. Lint subset
(tools/lint_subset, workspace): before 285 in 28 files, after 11 in 3 files (launcher.rs 7, overlay.rs 2, portmenu.rs
2, all DEVLOSS2); run files 274 → 0. Android (`clippy --lib --target aarch64-linux-android`) and wasm clippy: 0
outside the DEVLOSS2 files except render text.rs (3, wasm-only, not a wall target). Check: FAST arch_check 0
differing (emit-off 0), clippy wall ok (11 pending), cargo test 452 passed, tools/fmath_xplat 3 passed, spec_refs 0,
SPEC-GAP 1. Deviations: fmath.rs (23: `f64::from`, `cast_signed` / `cast_unsigned`, `is_multiple_of`, literal
separators — all `core`, fmath_xplat still includes it), d3dx.rs (2 test literals) and drawlist.rs (`#[expect]` on
`capture_screen`'s 640×480 `as u32`) are outside the row but no run owns them and the wall needs them at 0; the wall
is not yet `-D warnings` (CLIPPY_PENDING until DEVLOSS2 lands its files at 0, then set it empty in arch_check);
worldclone's test ECL builder uses `i16::try_from(..).unwrap()` / `i32::try_from` for instruction sizes (test-only,
values < 100).

**11p (1)–(2) done.** Files: crates/th08-core/src/{camera, menulist, drawlist}.rs, crates/th08-core/Cargo.toml (`image`
dropped), crates/th08-formats/src/{img.rs (new), lib.rs}, crates/th08-formats/Cargo.toml (`image` dev-dependency for
the vanilla test only), crates/th08-render/src/gfx.rs, Cargo.lock. (1) camera.rs: `rotation_x/y/z` on
`fmath::sin_cos_f`, `perspective_fov_lh` / `default_2d` / `set_2d` on `fmath::tan_f`; menulist `pie` on `cos_f` /
`sin_f`; both `allow(clippy::disallowed_methods)` and the `TODO(step 10)` gone. (2) `th08_formats::img::dims` (PNG IHDR
/ JPEG first SOFn, `ImgError`); `Recorder::load_image` reads the size from the header and emits
`ResCmd::CreateImage { id, bytes, w, h }` (same `TexSource::Image` provenance, label, error on a non-image header);
gfx `decode_image` decodes to RGBA8 once and keeps the pixels in the provenance for `rebuild` (a decode failure or a
size mismatch → transparent w×h + warning). anmmgr needed no change (it never called `load_image`). Public API:
new `ResCmd::CreateImage` (core, matched only in gfx), new `th08_formats::img::{dims, ImgError}`; `load_image`
signature unchanged. Lint subset (tools/lint_subset on the run files): 0 → 0; clippy wall clean. Tests: img
`png_header`, `jpeg_header`, `vanilla_images_match_decoder` (all 39 .jpg / .png of th08.dat: header size == decoded
size). Check: FAST arch_check 0 differing (emit-off 0, 456 passed); non-FAST arch_check **0 differing incl. draw
hashes** — the draw log prints vertices with `drawfmt::num`'s fixed decimals, so the f32 last-bit trig differences
never reach D; deliverable (3) has no new D to freeze. Images (master `target/release/th08` vs 11p, `TH08_SEED=1
TH08_TURBO=1`, offscreen; without both, two master runs already differ by up to 105 / 255 on 5 000 px): stages 1–7 at
frames 600 / 2400 (stage camera), spell-practice screens 16 / 17 with the pies (menu_mopup1 input, frames 425–605),
three more menu frames (title logo path) — all 22 pairs pixel-identical (largest difference 0), in
target/11p/. Deviations: none in code; the brief's expected D change does not appear.

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
