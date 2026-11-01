# Proposal A — idiomatic core

Status: proposal (2026-11-01). Scope: the cleanest idiomatic Rust for the port *as it is* — ownership, types, errors,
module boundaries, VMs, tests, build, performance, wasm. Builds on the baseline plan (workspace formats / core / render /
audio / app / tools, docs/PARITY_POLICY.md "Final idiomatic refactor") and refines it where measurement says the
baseline is not enough.

Thesis: the core is a **plain owned value** `Core` with no globals, no `Rc<RefCell>`, no platform handles. It is
stepped by `Core::frame(&mut self, &mut dyn Host) -> Flow`, polls input through the host, and *records* (does not
perform) drawing, sound and file writes. The original's funcChain stays — as an ordered list of enum keys over a
struct that owns every subsystem, not as `Rc<RefCell<dyn Task>>`. Everything else follows from those two decisions.

---

## 1. What src/ looks like today (measured, master e73440b)

51 150 lines in one crate `th08` (lib + main + 8 dev bins), 357 `#[test]`s, all in-module.

| Coupling | Count / where | Consequence for the split |
|---|---|---|
| `thread_local!` / `static` state in game logic | 19 cells: `sound::QUEUE`, `effect::{EFFECTS, LOADING_RELEASE}`, `gamemgr::{FLAGS, RANK_STATE, CURRENT}`, `bomb::QUEUED`, `ascii::{POPUPS, DARKNESS, GAUGE}`, `enemy::{TIMER_PAUSE, LOAD_FRAMES}`, `hud::{KEPT, PANEL}`, `anmvm::SLOWDOWN_RESTORE`, `gui::QUEUE`, `rng::{TRACE, CALLS}`, `scoredat::{VER_LIST, SAVE_DIR}`, `settings::SETTINGS` | hidden game state: two `Core`s in one thread would share it (rules out parallel tests, C's rollback, side-by-side diffing). Must become fields. |
| `Rc<RefCell<…>>` graph | 179 sites; `borrow()/borrow_mut()` 104 in enemy.rs, 45 eclvm, 33 hud, 27 bullet, 21 gamemgr. Owners: `PlayerRef`, `Stage`, `BulletManager`, `EnemyManager`, `Hud`, `Gui`, `Dialogue`, `EffectManager`, `GameManager`; `Weak` back-links bullet→enemies, player→bullets, ascii→player | runtime borrow panics instead of compile errors; state is not `Clone`; snapshot impossible. |
| Task chain | `Tasks<Ctx>` = `Vec<Node{prio, tag, Rc<RefCell<dyn Task<Ctx>>>}>` (task.rs) | dynamic dispatch over trait objects that each hold `Rc`s to siblings. |
| `Ctx` (game.rs) holds platform | `gfx: Gfx` (wgpu), `audio: Audio` (cpal), `pad: Joystick` (gilrs), `drawlog`, `settings: &'static` | core cannot compile without wgpu/cpal/gilrs. |
| `ctx.gfx` / `Gfx` use outside gfx/app | ~250 lines in 25 files (menu 32, hud 27, ascii 27, supervisor 22, stage 20, result 18 …). Calls: `set_viewport` 15, `free_texture` 14, `load_image` 7, `write_texture` 6, plus `sprite/strip/quad/fan/rect/image/clear_z/clear_viewport/capture_screen/viewport()` | drawing *and* resource management are immediate-mode calls into wgpu. |
| `AnmManager` | holds `TextureId`s, render caches (`last_blend`, `bound_tex`, `last_tex_matrix`), *and* the game RNG `rng`, `speed`, `screen_offset`, tint | logic and render state in one struct; RNG lives in the ANM manager. |
| Draw callbacks that write game state | measured writes in draw fns: `ascii.rs:1419 paused = 2` (11 §2), anmmgr caches, hud, stage, supervisor, result, player, musicroom; the main-menu description VM is run in menu draw too (12, "run and drawn on every screen") | the draw chain **is part of the simulation**; it cannot be moved out of core or made `&self`. |
| Platform reads in logic | `settings::get()` in 12 files (rng seed, noclip, se_log, clrd…), `Rng::boot()` reads wall clock / `js_sys::Date`, `scoredat::storage_read/write` (fs / localStorage), `supervisor.rs:464` and `menu.rs:196` poll the joystick, `input.rs` uses `winit::KeyCode` | input, time and storage must enter through one interface. |
| Audio from logic | `ctx.audio.*`: `stop_bgm` 9, `midi_reset` 5, `set_volumes` 4, `fade_bgm` 4, `play_bgm/play_slot/pause_bgm/load_bgm` 3 each, `play_se` via `sound::QUEUE` | all are commands, nothing is read back into logic → clean event stream. |
| Float math | `sin_f/cos_f/polar` (bullet.rs) = `f64::sin/cos` rounded to f32 (05 §10 x87 rule); 64 `.sin/.cos/.sqrt/.atan2` call sites across 12 files | result depends on the platform libm; one module must own it (shared with C). |
| `unsafe` | 3 (d3dx.rs, main.rs) | fine. |

Draw and calc are already separate *callbacks* (`Task::calc` / `Task::draw`); what is mixed is **state** (render caches in
AnmManager, GPU handles in logic structs) and **effects** (immediate GPU/audio calls).

## 2. Target architecture

### 2.1 Crates (baseline, refined)

```
th08-formats   pure parsers/writers: dat archive, anm, ecl, std, msg, sht, end, cfg, score.dat, rpy, thbgm.fmt,
               midi; typed errors (thiserror). No std::fs. Deps: encoding_rs, image(decode), thiserror.
th08-core      Core, World, VMs, task chain, rng, fmath, DrawList/SoundCmd/IoCmd types. Deps: th08-formats only.
               #![forbid(unsafe_code)], no cfg(target_arch), no wgpu/winit/cpal/gilrs/time/fs/log-to-stdout.
th08-render    wgpu backend: executes DrawList + ResCmd, presents, captures; text rasteriser (ab_glyph / canvas).
th08-audio     cpal mixer, thbgm streamer, SE bank, MIDI synth; executes SoundCmd.
th08           app: winit loop, Host impl, launcher (egui), settings, storage, native + wasm entry.
th08-tools     session/trace replay, drawlog (serialises DrawList), drawdiff, demosync, viewers, rpy batch runner.
```

`drawlog` moves *out* of core: it becomes a serializer of the recorded `DrawList`, so the core needs no logging mode.

### 2.2 Core API

```rust
pub struct Core { world: World, chain: Chain, res: ResTable, out: Outputs, cfg: CoreConfig, obs: Observers }

pub struct CoreConfig { pub seed: Option<u16>, pub noclip: bool, pub clear_masks: Option<ClearMasks>, /* debug only */ }

pub trait Host {
    fn poll_input(&mut self) -> RawInput;          // Input_Poll 0x43D970: keyboard bits + PadState, per poll
    fn wall_clock_ms(&mut self) -> u32;            // boot seed (11 §1.3) and nothing else
    fn read(&mut self, name: &str) -> Option<Vec<u8>>; // th08.dat members are in memory; this is for cfg/score/rpy
    fn text_raster(&mut self, req: &TextReq) -> Raster; // GDI text path (SPEC-GAP), pure function of the request
}

impl Core {
    pub fn new(assets: Assets, cfg: CoreConfig, host: &mut dyn Host) -> Result<Core, LoadError>;
    pub fn frame(&mut self, host: &mut dyn Host) -> Flow;     // Frame_Run 0x441E70: calc, draw if Continue
    pub fn outputs(&mut self) -> &mut Outputs;                // drained by the app after every frame
    pub fn world(&self) -> &World;                            // read-only, for traces / tools
}

pub struct Outputs {
    pub draws: Vec<DrawCmd>,   // this frame's device calls incl. Present markers (in-calc presents exist, 12 §1)
    pub res: Vec<ResCmd>,      // Create/Write/Free/CaptureScreen/CaptureRect texture, Load image
    pub sound: Vec<SoundCmd>,  // Se{id,pan,vol}, Bgm(Load|Play|Stop|Pause|Fade|Release), MidiReset, Volumes
    pub io: Vec<IoCmd>,        // Write{name, bytes} for th08.cfg, score.dat, replays
}
```

Why `Host` is a trait object and not a per-frame struct: the calc chain can **restart** and re-poll inside one frame
(00 §3, code 6), loading polls during in-calc presents, and the session tool injects a recorded value *per poll index*.
A trait call per poll keeps that exact; a `FrameInput` snapshot would not. `text_raster` must be a synchronous call
because ASCII/MSG/ending code writes the raster into textures in the same frame; it only affects pixels, never logic.

Texture ids: core owns `ResTable` (slot allocator, sizes, labels) and hands out `TexId(u32)`; render maps
`TexId → wgpu::Texture` as it executes `ResCmd`s. Nothing in core ever waits for the GPU. Screen captures
(`capture_screen`, `apply_pending_captures`) become `ResCmd::Capture{dst}` executed after `Present`, as now.

`viewport()` and the other device-state reads stay valid: core keeps a `DeviceState` mirror (viewport, z-test, fog,
blend cache) and appends commands — the renderer never answers questions.

### 2.3 Ownership: `World` + enum task chain

```rust
pub struct World {
    pub sv: Supervisor,              // Game (scene, cfg, stats, flags, score) — today ctx.game
    pub rng: Rng,                    // g_Rng; moves out of AnmManager
    pub anm: AnmManager,             // slots, scripts, speed, tint, screen_offset (no TexId caches → DeviceState)
    pub input: InputState,
    pub sound: SoundQueue,           // sound::QUEUE
    pub effects: Effects,            // effect::EFFECTS
    pub ascii: Ascii,                // POPUPS, DARKNESS, GAUGE, queue
    pub gm: Option<GameManager>,     // owns the stage-time subsystems below while a game runs
    pub menu: Option<MainMenu>, pub result: Option<ResultScreen>, pub ending: Option<Ending>, /* … */
}
pub struct GameManager { pub player: Player, pub bullets: BulletManager, pub enemies: EnemyManager,
                         pub stage: Stage, pub hud: Hud, pub gui: Gui, pub msg: Dialogue, pub items: Items, … }

#[derive(Clone, Copy)] pub enum Node { Supervisor, GameMgr, Player(Tag), Bullets, Enemies(Tag), Stage(Tag), Hud, … }
pub struct Chain { calc: Vec<(u16, NodeId, Node)>, draw: Vec<(u16, NodeId, Node)>, next: u64, dead: Vec<…> }
```

- The chain keeps FuncChain semantics exactly (insert after equal priority, on-register before link, restart, deferred
  on-remove `reap`) — only the payload changes from `Rc<RefCell<dyn Task>>` to a `Copy` enum. Dispatch is one
  `match node { Node::Enemies(t) => enemy::calc(&mut w, t), … }` in `chain.rs`.
- Cross-subsystem access uses **disjoint field borrows**: `enemy::calc(w)` destructures
  `let World { rng, anm, gm: Some(GameManager { enemies, bullets, player, .. }), .. } = w;` and passes exactly those.
  `Weak` back-links disappear: the bullet manager receives `&mut EnemyManager` as a parameter where it needs it.
- Re-entrancy (an on-register that registers other nodes, a calc that adds a node mid-walk) works because chain
  functions take `&mut World, &mut Chain` and the walk indexes by `NodeId`, as `task::run` does today.
- Where a callee needs the caller's own object *and* the world (ECL: enemy + manager), keep the existing take/put idiom
  (`eclvm::ecl_run`) and name it: `fn with_taken<T>(slot: &mut Option<T>, f: impl FnOnce(&mut T))`.
- `World: Clone` is a requirement from day one (cheap for C, needed for tests that fork a state). No `Rc`, no closures,
  no trait objects inside `World`. Script data (`Ecl`, `Anm` scripts) is shared immutable: `Arc<EclFile>` (Clone = refcount).

### 2.4 Types

Introduce only where they prevent real mistakes; arithmetic stays the original's.

| Type | Replaces | Rule |
|---|---|---|
| `Timer { prev, frac, cur }` | exists | keep; methods named after originals (`Timer_Tick`, `Timer_Greater`) |
| `Angle(f32)` `#[repr(transparent)]` | raw f32 radians in bullets/ECL/player | `wrap()` implements the one spec'd normalisation (07/08 quirks: `>= PI` vs `> PI` differ per site → separate named fns); `Add/Sub` are plain f32 ops; `.0` escapes are allowed. |
| `D3dColor(u32)` / `Bgra([u8;4])` | `color: [u8;4]`, ARGB shifts in 6 files | accessors `a() r() g() b()`; conversion fns once. |
| `BulletIdx(u16)`, `EnemyIdx(u16)`, `LaserIdx`, `ItemIdx`, `EffectIdx`, `AnmSlot(u8)`, `SeId(u8)`, `TexId(u32)` | `usize` indices | fixed arrays keep the original slot order (iteration order is logic); **no generational arenas** — they would change slot reuse. |
| `EclOff(u32)`, `AnmOff(u32)` | `ip: Option<usize>` byte offsets | ip stays a byte offset (jumps are byte offsets, traces show them). |
| Scene / difficulty / shot / stage enums | `i32`/`u8` (`sv.scene: i32`) | `#[repr(i32)]` enums with `TryFrom`; raw values remain in save/replay formats. |
| `GameFlags` bitflags | exists as newtype | use `bitflags` crate for flags words (GameFlags, AnmVm flags, ECL flag A/B). |
| `Score10(u32)` | score/10 integers | only if the HUD/score code uses both scales side by side (it does: `score10`, `score_disp`). |

Not newtyped: positions (`[f32; 3]` → a tiny `Vec3` with exact component ops is fine), fixed-point is limited to
`Timer` and `/256` sprite scale — no general fixed-point type is needed.

**fmath**: one module `th08_core::fmath` owns `sin_f`, `cos_f`, `polar`, `atan2_f`, `sqrt_f`, `hypot_f` with the
05 §10 / 11 §1.2 rounding rules and the spec citation on each. The 64 scattered call sites route through it; a
clippy `disallowed-methods` list (`f32::sin`, `f64::sin`, `f32::atan2`, …) in core enforces it. Whether `fmath`
calls `std` (platform libm) or the `libm` crate is decision **D4** below.

### 2.5 VMs (ECL / ANM / STD / MSG)

One shape for all four, matching what eclvm.rs already does best:

```rust
// formats: decoded once at load, byte offsets kept.
pub struct Ins { pub off: EclOff, pub time: i32, pub op: u16, pub rank_mask: u8, pub args: Args }
// core: one executor struct per run, borrowing exactly what the opcodes touch.
struct Exec<'a> { m: &'a mut EnemyManager, anm: &'a mut AnmManager, rng: &'a mut Rng, e: &'a mut Enemy, … }
impl Exec<'_> { fn exec(&mut self, t: &mut Thread, ins: &Ins) -> Step { match Op::from(ins) { … } } }
```

- Opcodes become `enum Op { Wait(i32), Jump { to: EclOff, .. }, SetVar(Var, i32), … }` decoded **lazily** per execution
  from `(op, args)` (`Op::from` is a cheap match) — the decoded enum makes each arm self-documenting, and the raw
  number stays for unknown/unused opcodes (`Op::Unknown(op)`; behaviour per spec). Decoding the whole file into
  `Vec<Op>` at load is also fine but must keep the offset → index map; pick one form for all four VMs (D6).
- Variable access (`int_read_10033…`) becomes `enum Var` with `read(&mut Exec)`; RNG-consuming reads stay methods
  that take `&mut Rng`, so "this read draws" is visible in the signature.
- Each opcode arm cites its spec row (`// 07 §8 op 85`), as today. Large arms move into named fns per op group
  (bullets, movement, spell, flow) in `ecl/ops_*.rs`.
- STD and MSG are already small `exec(&mut self, …)` matches: they only get the shared `Ins`/`Off` types.

### 2.6 Errors

- formats: `thiserror` enums per format (`ArchiveError`, `AnmError { Truncated{at}, BadMagic, … }`), no `anyhow`.
- core: `LoadError` (missing file, bad version list 15 §2.10, format error) from `Core::new` and from scene loads that
  read archive members; it maps to the original's error-quit (`Flow::Error`) path. **Runtime script execution never
  returns `Result`**: the original has no error path there; out-of-range indices follow the spec (clamp, wrap, ignore)
  and, where the spec says "undefined", `debug_assert!` + the documented fallback. No `unwrap()` on data from files.
- app/tools: `anyhow`.
- Panics are bugs; a `catch_unwind` boundary is not used (wasm panics abort anyway).

### 2.7 Observation, debugging, tools

Debug hooks become an `Observers` field in `Core` (plain data, `Option<…>`, off by default):
`on_restart` rows and `on_extra_present` (session.rs), the `rng` backtrace trace (`rng::TRACE`, native only, behind
`cfg(feature = "trace")`), SE log. Tools read state via `core.world()` (session `sample`); nothing in core writes files
or logs. `log` stays allowed in core for warnings only.

### 2.8 Tests

```
th08-formats/tests/        parse/round-trip on th08.dat members (skipped when TH08_DATA unset)
th08-core/src/**           unit tests in-module: rng, fmath, timers, chain semantics, VM opcode arms with tiny scripts
th08-core/tests/sessions.rs   golden, fullrun_noclip, selftest1, demos: full trace equality (headless, no GPU)
th08-core/tests/replays.rs    pilot set (22) under `cargo test`; ref500/ref_b2 via th08-tools batch (systemd units)
th08-render/tests/         DrawList → image on a software adapter (wgpu + lavapipe) for the draw groups
```

The 357 existing tests move with their modules. Trace tests become **port-vs-port** (PARITY_POLICY): a frozen
reference trace per session is generated at the pre-refactor commit and stored under `target/` or regenerated by tools —
no recorded files are touched. Core tests need neither GPU nor audio, which also allows `cargo test` in CI.

### 2.9 Build times, performance, wasm

- Today every edit to logic recompiles a crate that links wgpu/egui/cpal. After the split, `cargo check -p th08-core`
  depends only on formats + encoding_rs + thiserror; expect seconds for incremental checks (measure at step 3;
  agents can use `-p th08-core` for every logic IMPL). wgpu only rebuilds when render changes.
- Performance: enum dispatch + direct field access removes ~180 `RefCell` borrow checks per frame and the vtable hop;
  `DrawCmd` buffers are reused (`clear()`, no per-frame allocation); the renderer batches as today. Headless replay
  speed (ref500 batches) should improve because headless runs skip the GPU entirely (today they still drive `Gfx`).
- wasm: core contains no `cfg(target_arch)`; the two existing wasm forks in logic (boot clock, storage) move to `Host`.
  The app keeps `wasm_bindgen` start, canvas text raster and localStorage. Check: `cargo build --target
  wasm32-unknown-unknown -p th08` in every step.

## 3. Mapping src/ → crates

| Today | Target | Untangle first |
|---|---|---|
| archive, anm (file), ecl (file), stdfile, sht, config (file part), replay (file part), scoredat (file part), midi (parser), d3dx (image decode), msg (file part), ending (file part) | th08-formats | split file structs from runtime state in replay/scoredat/msg/ending; `scoredat::VER_LIST` → value passed in |
| rng, task, game (Ctx→Core), supervisor, gamemgr, player, pshot, bomb, bullet, item, enemy, eclvm, stage, anmvm, anmmgr (minus GPU), effect, hud, gui, ascii, msg, menu, menulist, result, ending, musicroom, sound, input (state machine) | th08-core | §1 table: globals → fields, Rc graph → World, gfx calls → DrawCmd, audio → SoundCmd, settings → CoreConfig |
| gfx, text (raster), anmmgr device caches → DeviceState mirror in core + executor in render | th08-render | `Gfx` API surface becomes `DrawCmd` variants 1:1 |
| audio, midi (synth) | th08-audio | `Audio` methods become `SoundCmd` variants 1:1 |
| app, main, launcher, settings, assets, joystick, input (winit keymap), lib.rs wasm start, scoredat storage | th08 (app) | `Host` impl |
| session, demosync, drawlog, drawreport, anmdebug, xz, bin/* | th08-tools | read `core.world()`; drawlog serialises `Outputs.draws` |

## 4. Migration plan (each step one commit, each verified)

Checks: **T** = verify sessions (golden, fullrun_noclip, selftest1, demos) full + pilot 22; **B** = ref500 490/490 and
ref_b2 498/498 (systemd batch, after steps marked B); **D** = golden draw log identical to the step's reference (port vs
port; draw-only steps may instead pass an image diff); **W** = wasm build. Every step runs T + W.

0. **Freeze references.** At the start commit: write port traces + draw logs for all sessions and pilot (port-vs-port
   baseline). Add `tools/arch_check` script running T/D. *Check:* reruns reproduce themselves.
1. **Workspace shell.** Cargo workspace with the existing crate as `th08`; empty `th08-formats`/`th08-core`. *T, D, W.*
2. **formats out.** Move pure parsers; `th08` re-exports. Typed errors. *T, D, W, unit tests.*
3. **Globals → fields (still inside th08).** One commit per cluster: sound queue; effect EFFECTS/LOADING_RELEASE;
   gamemgr FLAGS/RANK_STATE/CURRENT; ascii POPUPS/DARKNESS/GAUGE; hud KEPT/PANEL; enemy TIMER_PAUSE/LOAD_FRAMES;
   bomb QUEUED; anmvm SLOWDOWN_RESTORE; gui QUEUE; settings → `CoreConfig`. *T, D each; B after the cluster.*
4. **RNG out of AnmManager** into `World.rng` (pure move; call order untouched). *T, D.*
5. **fmath module** + clippy `disallowed-methods`; route all 64 sites. Same functions, no numeric change. *T, D, B.*
6. **Sound → SoundCmd.** `Audio` methods called by logic become commands drained after each frame (same order). The
   frame-tied BGM fade state machine stays in core. *T; listen test; SE log identical.*
7. **Storage/time/input → Host.** `scoredat::storage_*`, `Rng::boot`, joystick polls, winit keymap move to the app;
   input polled via `Host::poll_input`. *T (session tool now implements Host with recorded polls), W.*
8. **Gfx → DrawList.** Introduce `DrawCmd` mirroring today's `Gfx` calls 1:1, a `Recorder` with the same method names,
   and `DeviceState`; switch call sites file by file (menu, hud, ascii, stage, supervisor, result, effect, …);
   `Gfx` executes the list. Resource calls → `ResCmd` with core-allocated `TexId`. *D after each file, T.*
9. **Rc graph → World.** Bottom-up: Player (`PlayerRef`) → BulletManager (drop `Weak`) → EnemyManager → Stage →
   Hud/Gui/Dialogue → EffectManager → GameManager; the chain payload becomes `Node`. One subsystem per commit; the
   old `Task` impl forwards to the new fns until the last one. *T, D each; B after GameManager.*
10. **Crate cut.** Move core modules into `th08-core`; `forbid(unsafe_code)`, no wgpu/cpal deps — the compiler now
    proves the boundary. render/audio/tools crates. *T, D, B, W.*
11. **Idiomatic pass per module** (types of §2.4, VM `Op` enums of §2.5, clippy pedantic subset, dead SPEC-GAP paths
    removed). Small commits; arithmetic/call order frozen; quirks into named helpers. *T, D per module; B weekly.*
12. **Renderer rewrite (optional)**: free as long as images match (PARITY_POLICY). *Image diff of draw groups.*

Order rationale: globals and effects first (mechanical, low risk, unlock everything else), Rc graph late (largest,
touches enemy.rs's 104 borrow sites), crate cut only when the boundary already holds so the cut is a pure move.

## 5. What A requires of the core — decide now vs later

**Decide at the start (expensive to retrofit):**

- **D1 Core is a value**: no globals, `World: Clone`, no `Rc`/`RefCell`/trait objects/closures in state. Every later
  step (tests in parallel, C's snapshots, B's pack switching) depends on it.
- **D2 Draw chain lives in core** and records `DrawCmd`s; draw callbacks keep `&mut World` because they write logic
  state (paused = 2, popup offset reset, menu VMs). The renderer is a pure executor and never answers queries.
- **D3 Host trait with per-poll input** (restart passes and in-calc presents poll again), plus wall clock (boot seed
  only), file read, text raster. The session tool, live play, wasm and C's netplay are all Host impls.
- **D4 fmath backend**: keep `std` f64 sin/cos (bit-exact on x86-64 Linux today, verified by 1010 replays) or switch to
  the `libm` crate (same result on every target, must re-run B to prove equality with the original). A recommends
  the `libm` crate behind the `fmath` facade *if* B passes; otherwise std on native and a documented risk for
  wasm/ARM. Coordinate with C; the facade itself is mandatory now.
- **D5 Index model**: fixed arrays + typed indices, no generational arena; slot order is logic.
- **D6 VM instruction form**: byte-offset ips kept; one decode style (lazy `Op::from` vs pre-decoded with offset map)
  for all four VMs.
- **D7 Texture ids owned by core** (`ResTable`), resources created by command.

**Can be added later:** newtypes beyond indices (`Angle`, `D3dColor`, enums for scene/difficulty), `Op` enums per VM,
error enum refinement, render rewrite, `Observers` beyond the session hooks, CI, clippy pedantic, docs.

## 6. Compatibility, risks, effort

**With B (modding).** Shared: assets come through one interface (`Assets` given to `Core::new`; B's VFS overlay sits
behind it, core never knows); `Host::text_raster` is exactly the hook B needs for replaceable fonts — the GDI text
path is already outside the bit-exact zone. String tables (msg, menus, spell names) parsed in formats are the natural
replacement point. Conflict: if B wants hot reload of *scripts* (ECL/ANM), `Arc<EclFile>` swap must happen between
frames — compatible. Vanilla stays bit-exact because overlays are empty.

**With C (netplay).** D1 is C's snapshot requirement (`World: Clone` ⇒ rollback = clone; hashing = a `Hash`-like
walk or serialise). D3 gives input-only sync (`Host::poll_input` from the network). D4 is C's cross-platform
determinism question — the `fmath` facade is where C's choice lands. Potential conflict: C may want `World` as
flat POD (memcpy snapshots, no `Vec`); A keeps `Vec`s for VM scripts/threads → `Arc` for immutable data and bounded
`ArrayVec`/fixed arrays for mutable pools satisfy both. The draw-in-core rule (D2) also lets C skip drawing during
rollback re-simulation **only** if draw-phase writes are first isolated — D2 should record which draw fns write
logic state (§1 list) so a `draw_logic_only` path can be added later.

**Risks.**
- Draw-phase logic writes are easy to miss when converting draw fns to recording; caught by T (paused/popup timing).
- Converting `Weak`/`Rc` order of operations: `RefCell` today serialises access implicitly; with field borrows an
  operation that read a sibling *before* a write may now read after — keep each fn's statement order, T catches it.
- Renderer device-state queries (viewport) must be mirrored exactly; D catches it.
- Agents converting 3.5k-line files: enemy.rs/eclvm.rs need sub-steps (by opcode group).
- `libm` switch (D4) could change last bits; gated on B.

**Effort (agent runs of ≤3 deliverables, T per run, B per cluster).** Steps 0–2: 3 runs. Step 3: ~5 runs. 4–5: 2.
6–7: 3. Step 8: ~6 runs (one per draw-heavy file group). Step 9: ~7 runs. Step 10: 2. Step 11: ~15 runs (one per
module group). Total ≈ 43 IMPL runs, of which the architecture (0–10) ≈ 28; step 11 can interleave with B/C work once
step 10 has landed.
