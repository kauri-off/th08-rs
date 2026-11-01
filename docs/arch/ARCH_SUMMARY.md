# Architecture proposals — comparison and recommendation

Inputs: [ARCH_A_IDIOMATIC.md](ARCH_A_IDIOMATIC.md) (idiomatic core), [ARCH_B_MODDING.md](ARCH_B_MODDING.md) (content /
mod platform), [ARCH_C_NETPLAY.md](ARCH_C_NETPLAY.md) (netplay-ready deterministic core). Brief:
docs/briefs/ARCH_PROPOSALS_BRIEF.md.

## Verdict
The three are not alternatives: they agree on the core's shape and differ only in what they add on top. Recommended:
**A's migration plan as the skeleton, with B's content boundary and C's determinism rules adopted as day-0 decisions.**
Mod features (B) and netplay (C) are then later, additive projects; nothing in A has to be redone for them.

## Agreed by all three
- One owned `World` value: no `thread_local!`/statics (14–19 today), no `Rc<RefCell>`/`Weak` (~180), no trait objects
  holding state; cross-references by slot index / id.
- Fixed-capacity pools keeping the original slot order and free-slot search (slot order is game logic).
- Core does no GPU/audio/file IO: it emits draw commands, texture commands (core-owned ids), sound events, file writes;
  clocks, seeds, input, file reads come in through one `Host` interface (input per poll, not per frame).
- One `fmath` module for all transcendental math with a clippy ban on direct float calls elsewhere.
- Logic assets (ECL/STD/SHT/MSG, ANM sprite rects + scripts, .end) separated from presentation assets (pixels, display
  text, fonts, samples); ANM parsing split into logic data and pixels.

## Conflicts and resolutions
1. **Draw writes logic state** (A: `paused = 2`, popup reset, menu VMs run in draw → draw must keep `&mut World`;
   C: wants `draw(&self)` so rollback can skip drawing). Resolution: each draw fn is split into a *draw-phase step*
   (`&mut World`, part of the simulation, always runs once per presented frame — also headless, in rollback re-sim and
   in tools) and a pure *emit* (`&self` → draw commands). C's "draw-off" test becomes "emit-off". A's step 1 inventories
   the draw-phase writes (A already proposes recording them).
2. **Text type** (A would like `String`; B needs raw Shift-JIS because score.dat checksums, replays and line timing use
   bytes). Resolution: `SjisStr`/`SjisString` newtype over bytes in core and formats; decoding only on the render side.
3. **Math** (A: std vs `libm` undecided; C measured that std `atan2`/`tan`/`acos` differ between Linux, wasm and
   windows-gnu on 8–16 % of inputs, sin/cos match; pinned `libm` in f64 → f32 matches today's Linux results on all but
   4 sampled inputs; `rsqrtss` reproducible with a ~32 KB table). Resolution: adopt pinned `libm` + the rsqrt table
   (C0) **before** the rewrite, accepted only if verify sessions, pilot and ref500/ref_b2 stay exact.

## Day-0 decisions (expensive to retrofit)
| # | decision | from |
|---|---|---|
| 1 | `World` is a plain value: no globals, no `Rc`/`RefCell`/`Weak`, no state in trait objects/closures; `Clone` | A D1, C 1 |
| 2 | draw = draw-phase step (`&mut`) + emit (`&self`); render caches live in the renderer, never restored by rollback | A D2, C 2 (resolved) |
| 3 | `Host` trait: input per poll, clocks/seeds/files/settings as inputs; sounds/saves/texture loads as output events | A D3, C 3, B D6 |
| 4 | pinned `fmath` (libm + rsqrt table) and the clippy ban, from the first step | A D4, C 4 |
| 5 | fixed pools with original slot order; typed slot indices | A D5, C 5 |
| 6 | core reads content only through a `LogicView`; no GPU/audio handles or archive paths in core state; texture ids owned by core | B D1–D2, A D7, C 7 |
| 7 | text drawing is an output command (never during calc — today `hud.rs` uploads dialogue text in calc); `SjisStr` in core | B D3–D4 |
| 8 | per-frame input is a per-player `u16` array; player-indexed state with `players[0]` in vanilla | C 6 |
| 9 | logic-content fingerprint (empty for vanilla) in session/trace metadata | B D5, C 7 |
| 10 | one instruction-decoding style for the four VMs | A D6 |

Deferred (additive, after the rewrite): opcode enums and extra newtypes, renderer rewrite, VFS overlay packs, `.po`
string tables, pack fonts, HD textures, hot reload, gameplay packs, rollback session layer, spectating, co-op/versus.

## Proposed order
1. **C0 math fix** in today's code (one small IMPL + full long check): pinned `libm`, rsqrt table, `fmath` module.
   Also gives a bit-exact wasm build.
2. **A steps 0–12** with decisions 1–10 written into the step briefs (B's M0–M3 and C's C1–C4 fall inside them: core
   without platform calls, content views, draw split, inputs/outputs). Each step: verify + pilot; batch steps also
   ref500/ref_b2 and the port-vs-port draw log; wasm build.
3. Extra checks introduced during the rewrite, cheap to add: emit-off run (logic traces identical with emit disabled),
   clone/restore round-trip test, wasm trace in node equals native, "chaos pack" (presentation garbage → logic exact).
4. After the rewrite, as separate projects if wanted: B's pack system and translation tooling; C's rollback netplay.

Effort: A ≈ 43 implementer runs (≈ 28 for the architecture itself); B's and C's day-0 items add only a few runs
because they fall into A's steps; their full features are separate projects.
