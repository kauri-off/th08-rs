# Rewrite plan (chosen 2026-11-01)

Skeleton: ARCH_A_IDIOMATIC.md §4 steps 0–12. Day-0 rules: ARCH_SUMMARY.md table (decisions 1–10), i.e. A + B's
logic/content boundary + C's cross-platform determinism. One IMPL per step (≤ 3 deliverables); clusters split further.

Checks: **T** verify.zsh sessions + pilot 22; **D** port-vs-port draw log identical to the step-0 baseline (or the
previous step's, when a step intentionally changes draw); **B** ref500 490/490 + ref_b2 498/498 (orchestrator, systemd);
**W** wasm build. Every step T + W.

| step | content | extra rules from B / C | checks | status |
|---|---|---|---|---|
| C0 | fmath (pinned libm) + AMD rsqrt table + clippy wall | C 4 | T B W | done IMPL150 |
| 0 | freeze port-vs-port references (traces + draw logs of all sessions + pilot); `tools/arch_check` (T + D) | — | T D | |
| 1 | Cargo workspace shell: `th08` + empty `th08-formats`, `th08-core`; `default-run` app | — | T D W | |
| 2 | formats out (pure parsers, typed errors); text stays `SjisStr` bytes | B D4 | T D W | |
| 3 | globals → fields, one cluster per commit (sound queue; effect; gamemgr; ascii; hud; enemy; bomb; anmvm; gui; settings → CoreConfig) | C 1 | T D, B per cluster | |
| 4 | RNG out of AnmManager into World.rng | C 3 | T D | |
| 6 | sound → SoundCmd (by id/name; track lookup before any override) | B D6, C 3 | T, SE log | |
| 7 | storage/time/input → Host (input per poll; per-player u16 array, players[0]) | C 3, C 6 | T W | |
| 8 | Gfx → DrawList + ResCmd with core-owned TexId; text drawing becomes an output command (no texture writes in calc); draw fns split into draw-phase step (&mut) + emit (&self) | B D2–D3, C 2 | D per file, T | |
| 8b | content views: core reads only LogicView (no Archive/paths in core); ANM parsed into logic data + pixels; logic fingerprint in session metadata | B D1, D5 | T D | |
| 9 | Rc graph → World (Player → Bullets → Enemies → Stage → Hud/Gui/Dialogue → Effects → GameManager) | C 1, C 5 | T D, B at end | |
| 10 | crate cut: th08-core (forbid unsafe, no wgpu/cpal), render, audio, tools | — | T D B W | |
| 11 | idiomatic pass per module (types, VM Op enums, clippy subset, dead paths) | A D6 | T D per module, B weekly | |
| 12 | optional renderer rewrite | — | image diff | |

Mobile (ARCH_D_MOBILE.md; Android APK + mobile web, no iOS) adds to the steps above:
- every step: W also builds `th08-core`/`th08-formats` for `aarch64-linux-android`;
- 7b (after IMPL158): one merged app-side input source (keyboard/gamepad/touch → input word); save + settings dirs
  passed in by the entry point; frame pacing split from the window code (suspend/resume, no sleep loop);
- 8: GPU device separate from the surface (drop/recreate); texture provenance list; touch overlay drawn after scaling,
  never into the 640×480 frame;
- 8b: game data through an app-side source trait (folder / APK / web);
- 9: saves outside the original's save points use a World clone (score.dat write uses the RNG);
- 10: app crate builds as cdylib with desktop/Android/web entries; rfd/clap/env_logger desktop-only;
- after 10: Android runs (≈6) then mobile web (≈2); local APK script with bundled assets;
- touch overlay is user-editable like other mobile ports: in-app layout editor (drag/resize each control, opacity,
  show/hide, per-orientation profiles, reset to default), stored in port settings (+1 run).

New tests introduced along the way: emit-off run (logic traces identical with emit disabled, after 8); World
clone/restore round trip (after 9); wasm trace under node equals native (after 10, see IMPL150_QUESTIONS.md);
chaos pack (after 8b).
