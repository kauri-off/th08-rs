# IMPL150 questions

No new SPEC-GAP. fmath implements the 05 §10 rule (extended x87 result rounded once to f32) with the pinned
`libm` in f64, and `rsqrt_amd` is the "exact model of that CPU's table" 05 §10 allows.

## Plan: wasm32 session trace test (deliverable 3, not done in this step)

Done here: unit-level cross-target check `tools/fmath_xplat/run.sh`. It includes `src/fmath.rs` unchanged,
evaluates all 10 fmath functions on 2²⁰ inputs each (half gameplay range, half arbitrary bit patterns), and
compares per-chunk digests of wasm32-unknown-unknown under node with native x86_64. Result: 160/160 chunks
identical. Only transcendentals and rsqrt are platform-dependent, so this covers the known risk.

A full `selftest1` / `golden` trace run as wasm needs a larger harness:

1. The session replay (`src/session.rs`, `src/bin/session.rs`) uses `std::fs` for the session dir, the save dir
   and the trace. wasm32-unknown-unknown has no fs, and the wasm32-wasip1 target is not installed here.
   Option A: install wasm32-wasip1 and run under node's `node:wasi` (preopened dirs). This needs the session
   path to compile without wgpu/winit/cpal: a `headless` cargo feature or a separate core crate (a split into
   a `th08-core` crate). That split is the natural place for it.
   Option B: wasm32-unknown-unknown plus a JS shim that passes the archive bytes, polls and events in memory
   and returns trace rows. This needs an in-memory `assets::Source` and a session entry point that takes bytes.
2. Draw stays off (`--no-drawlog`); the replay must not create a wgpu device. Today `session` needs a `Gfx`
   for the draw calls. A null renderer, or a `step`/`draw` split, removes that.
3. Compare: node writes trace.txt; `session::compare` (or `diff`) against the native trace, all rows.

Suggestion: do it right after the core split (C1), when `th08-core` builds without the renderer. Then add
`tools/wasm_session/run.sh` to verify.zsh.
