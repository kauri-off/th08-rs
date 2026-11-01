# ARCH C — netplay-ready deterministic core

Proposal C of docs/briefs/ARCH_PROPOSALS_BRIEF.md. Question: what must `th08-core` look like so that rollback
netcode, spectating / live replays, optional 2-player modes, and *cross-platform* determinism can be added later
without reopening the core. Vanilla single-player stays bit-exact (docs/PARITY_POLICY.md). Every number below was
measured on master e73440b (scratch code in target/arch_C/{sizes,fmath,xplat}); nothing in src/ was touched.

## 0. Measured facts that drive the design

| # | Fact (measured) | Consequence |
|---|---|---|
| F1 | **The port is not cross-platform deterministic today.** `f32::atan2`, `f32::tan`, `f32::acos` call the platform libm. On Linux they come from glibc 2.44 (correctly rounded, CORE-MATH). Built for wasm32 and run in node, they differ from Linux on **16.1 % / 14.0 % / 8.6 %** of 2·10⁶ random gameplay-range inputs. The x86_64-pc-windows-gnu build (run under wine) gives 16.1 % / 14.0 % / 7.6 %. | The wasm build cannot be bit-exact with the 1010 replays today. A pinned math module is needed now, not only for netplay. |
| F2 | f64 `sin`/`cos` rounded to f32 (the 05 §10 model): the `libm` crate 0.2.16 equals glibc on **all 2.18·10⁹ f32 inputs with \|x\| ≤ 8**. On wasm and windows-gnu, 0 of 2·10⁶ samples differ. | 05 §10 sin/cos are already portable. Pin them to `libm` anyway. |
| F3 | `libm::{atan2,tan,atan,acos}` in f64, then rounded to f32, equal glibc's f32 result on 5·10⁷ atan2 pairs (0 diffs) and on all f32 with \|x\| ≤ 8 for tan (0 diffs), atan (2 diffs) and acos (2 diffs). `libm`'s own f32 versions (`atan2f`, …) differ in millions of cases. | Core rule: every transcendental = `libm` f64 → f32. This reproduces today's Linux results; replay batches decide the 4 corner inputs. |
| F4 | `D3DXVec3Normalize` uses native `rsqrtss` (src/d3dx.rs, 05 §10). On the reference CPU (Ryzen 9 7945HX) the output has 12 significant bits (low 11 bits always 0) and obeys rsqrt(4ᵏx) = 2⁻ᵏ·rsqrt(x) exactly (2.2·10⁷ checks, 0 mismatches). Over [0.5, 2) the output has **4080 runs**. | A **≈ 32 KB table** reproduces AMD's `rsqrtss` exactly on Intel, ARM and wasm. Today, non-x86 builds use 1/sqrt, and Intel CPUs have a different rsqrtss table. Both are off-reference. |
| F5 | Plain f32 `+ − × ÷ sqrt` and `as` casts are IEEE and the same on x86-64, aarch64 and wasm. Rust never contracts to FMA, and there are 0 `mul_add` in src/. `powi(2)` == x·x on 10⁷ inputs. | No x87 emulation is needed: the 24-bit-precision model (05 §10) is already plain f32. Only transcendentals and rsqrt are platform-dependent. |
| F6 | Live game state is spread over **20 `thread_local!` statics** (+ the `OnceLock` Settings): 14 hold logic state (effect `EFFECTS`, bomb `QUEUED`, gamemgr `FLAGS`/`RANK_STATE`/`CURRENT`, enemy `TIMER_PAUSE`/`LOAD_FRAMES`, anmvm `SLOWDOWN_RESTORE`, effect `LOADING_RELEASE`, ascii `POPUPS`/`DARKNESS`/`GAUGE`, sound `QUEUE`, gui `QUEUE`). An `Rc<RefCell>` graph links GameManager → {Stage, BulletManager, EnemyManager, Hud}, Player ↔ BulletManager (Weak) ↔ EnemyManager (Weak), and ascii GAUGE → Player (Weak). The task chain is `Rc<RefCell<dyn Task<Ctx>>>`. | Snapshot = `clone()` of one value is impossible today. So are two worlds in one process (versus mode, parallel tests). |
| F7 | Logic state sits in render-ish owners. `AnmManager` holds the game RNG next to blend/texture caches and loaded GPU slots. Stage/ANM loading in calc goes through `&mut Gfx` (`ctx.anm.load(&mut ctx.gfx, …)`). The calc-side modules have 2–63 `gfx` references each. All 17 `Task::draw` take `&mut self`. | The calc/draw split and the asset/GPU split must be cut before snapshots are possible. |
| F8 | Sizes: Bullet 3560 B × 1536, Item 632 × 2096, Effect 736 × 653, Laser 1232 × 256 = **7.6 MB of fixed pools**. Enemies are boxed and live-only (6864 B each). Player 7.2 KB, Stage 3.4 KB. `clone_from` of the four pools takes **0.26 ms** (29 GB/s, native). | A naive full-copy snapshot every frame is affordable natively (8-frame ring ≈ 70 MB, ~0.3 ms/frame). wasm and low-end ARM want live-slot-only copies (§1.4). |
| F9 | Non-determinism sources inside logic today: `Rng::boot()` (wall clock, 11 §1.3, also on restart), `web_time::Instant` play/run clocks in gamemgr, `settings::get()` reads in 14 core modules (debug flags such as noclip, seed), `storage_write` in scoredat/result/replay/game. Saving consumes 2–3 RNG draws (15 §2, §3), so the save step is logic. | All of these become explicit inputs/outputs of the core. |
| F10 | Input is already a `u16` word per frame (09 §3, 15 §3.2). Gameplay reads g_Input, which the prio-17 recording node writes at frame end (1-frame latency). The replay stream *is* that word. | Netplay input = the replay stream: 2 bytes/player/frame. A live replay and a spectator feed are the same thing. |

## 1. Target architecture

Baseline crates stay (`th08-formats`, `th08-core`, `th08-render`, `th08-audio`, `th08`, `th08-tools`). C adds:

- `th08-core::fmath` — the only place transcendentals and rsqrt live (F1–F4).
- `th08-core::world` — the single owned `World` (all logic state). `Clone`, no pointers, no interior mutability.
- `th08-session` (new, no IO): match setup, input queues, rollback / spectate / replay drivers. It works on the core API
  only. It is pure, so tools and tests can drive it without sockets.
- `th08-net` (new, later): transport (UDP + optional WebRTC for wasm), packet format, NAT/relay. It depends on
  `th08-session`, never on the core internals.

### 1.1 Core API (sketch)

```rust
pub struct Core { world: World, assets: Arc<Assets> }      // Assets: parsed, immutable, shared by snapshots

pub struct FrameInput {                                      // everything logic may observe in one Frame_Run
    pub pads: [u16; MAX_PLAYERS],                           // raw input word per player (09 §3); vanilla uses [0]
    pub focus_lost: bool,                                    // GetDeviceState failure stand-in (15 §1.1)
}
pub struct BootInput { pub seed: u16, pub cfg: Config, pub score_dat: Option<Vec<u8>>, pub flags: CoreFlags }

impl Core {
    pub fn boot(assets: Arc<Assets>, b: BootInput) -> Self;          // WinMain start-up (00 §2) minus IO
    pub fn step(&mut self, i: &FrameInput, out: &mut Events) -> Flow; // one Frame_Run calc half incl. code-6 passes
    pub fn draw(&self, ds: &mut DrawState, list: &mut DrawList);      // draw half: &self, writes only DrawState
    pub fn save_state(&self, s: &mut Snapshot);                       // clone_from into a reused buffer
    pub fn load_state(&mut self, s: &Snapshot);
    pub fn hash(&self, level: HashLevel) -> u64;                      // Canary (cheap) | Full
}
pub enum Event { Se { id: u8, pan: i8 }, Bgm(BgmCmd), Save { file: SaveFile, bytes: Vec<u8> },
                 NeedRestartSeed, PlayClock(ClockCmd) /* host-side wall clock */ }
```

- `step` takes everything nondeterministic as an argument. Boot seed, restart seed (via `NeedRestartSeed` → the next
  `FrameInput`/`BootInput`) and loaded files are inputs. Saves, SE and BGM are events. The core never reads a
  clock, a file, a global setting or the GPU.
- `draw(&self, …)`: draw-side writes (HUD "last value shown" +0x28 12 §5.4, bullet VM pos/rotation set before
  drawing 08 §2, result pos-offset restore (16, `Result_Draw`), blend/texture caches 05 §12.5/§12.8, HUD panel cache) go to
  `DrawState`, which the renderer owns and which is **not** snapshotted. Values the draw only *computes* are not
  stored at all.
- `Flow` and the funcChain (00 §3) stay inside `step`. The chain becomes `Vec<Node { prio, tag, kind: TaskKind }>`
  with `TaskKind` a plain enum and the task data in `World` fields (`world.bullets`, `world.enemies`, …). Call
  order is unchanged: the runner walks the same sorted list, and code 6 restarts from the head inside the same
  `step`.

### 1.2 Determinism layer (`fmath`)

```rust
// 05 §10: extended fsin/fcos rounded once to f32; pinned libm f64 so every target computes the same bits.
pub fn sin_f(a: f32) -> f32 { libm::sin(a as f64) as f32 }
pub fn polar(a: f32, r: f32) -> [f32; 2] { let (a, r) = (a as f64, r as f64); [(libm::cos(a)*r) as f32, (libm::sin(a)*r) as f32] }
pub fn atan2_f(y: f32, x: f32) -> f32 { libm::atan2(y as f64, x as f64) as f32 }
pub fn rsqrt_amd(s: f32) -> f32 { /* 4080-run table over [0.5,2) + exponent shift, generated on the reference CPU */ }
```

- Lint wall in `th08-core`: `#![deny(clippy::disallowed_methods)]` with clippy.toml listing `f32::{sin,cos,tan,
  atan,atan2,acos,asin,exp,ln,powf,sin_cos,hypot,mul_add}` and the f64 equivalents, plus `std::arch` and
  `std::time`. Also `#![allow(clippy::suboptimal_flops)]`, because that lint suggests `mul_add`.
- The rsqrt table: a `th08-tools` generator dumps it on the reference CPU (x86_64 only). The checked-in
  `rsqrt_amd.bin` (~32 KB, or ~8 KB as (run start u32, 12-bit mantissa u16)) gets a unit test. The test compares
  the table with native `rsqrtss` when it runs on the reference CPU, and checks the spec example of 05 §10
  everywhere. This is spec-backed (05 §10 says "or an exact model of that CPU's table elsewhere") and needs no new RE.
- NaN: wasm may produce NaN payloads that differ from x86. NaNs give the same comparison results everywhere, so
  logic is unaffected, but `hash()` canonicalises NaN before hashing f32 fields.
- `HashMap` iteration order: not used in logic today (archive lookup only). It stays banned in `World`
  (`BTreeMap`/index vectors only).

### 1.3 Data flow per frame

```
offline / replay:   host polls pad → FrameInput{pads:[w,0]} → core.step → events → audio; core.draw → render
                    (replay: pads[0] from the .rpy stream as today, prio-6 playback node unchanged)
rollback netplay:   session.add_local(frame, w); session.add_remote(...)
                    loop over session.requests():  Save(f) → core.save_state(ring[f%N])
                                                   Load(f) → core.load_state(ring[f%N])
                                                   Advance(inputs) → core.step(inputs, &mut events_for(f))
                    events: SE played only the first time frame f is simulated or when a resimulated f emits an
                    SE id the first run did not (GGPO rule); BGM commands applied from confirmed frames only
                    render: core.draw once per displayed frame (never during resimulation)
spectator / live replay: a stream of (frame, pads) + every 60 frames (frame, hash Canary) → plain replay playback
```

Netplay runs from a **match start**, not from boot. The `MatchConfig` is the replay header data (15 §3:
shot/team, difficulty, stage, seed, rank, `cfg_game` bytes, spell-practice id), plus the core version, the logic-asset
hash (B's content packs included, §5) and the `CoreFlags` (noclip, slowdown). Both peers build the World with the
same code path that replay playback uses for a stage start (`replay::play_stage_load`, 15 §3.8). Menus before and
after run locally. PAUSE (bit 3) and the in-game pause menu are host commands in netplay: both peers stop and the
bit is masked from the synced word. SKIP (bit 8, dialogue) and everything else stay in the synced word.

### 1.4 Snapshots and hashing

- `Snapshot` = `World` itself, cloned with `clone_from` into a ring of preallocated Worlds. Copying from a warm
  ring allocates nothing.
- `Pool<T, const N>` (fixed-capacity slot array + live bitset + the original's free-search order) gives a
  `clone_from` that copies only live slots. With typically < 200 live bullets this cuts F8's 7.6 MB to well under
  1 MB. The slot index stays the original's index (spawn order and quirks depend on it, 08 §2, 13 §2.1).
- Heap inside entities (enemy ECL call stacks `Vec<Context>`, `Box<Thread>`, `stored_fire`) is allowed, but
  `clone_from` must reuse capacity. Later this can become a fixed `ArrayVec` with the original's limits (07 §5),
  which turns Enemy into POD.
- `hash(Canary)` = existing trace-row data (RNG state + `total` draws, player pos, score, counts; session.rs Row).
  It costs nothing and catches nearly every desync within frames, because almost all divergence perturbs the RNG.
  `hash(Full)` = a `StateHash` visitor derived over `World`. It is used every N frames and after a Canary mismatch,
  and dumps a per-subsystem hash list so both sides can bisect.
- Serialization (late join, desync dumps, "save state" feature): `serde` behind a feature on `World`. Format
  version = core version. No pointers in `World` makes this mechanical. **Decide now: indices, not references.**

### 1.5 Two-player modes (extension; vanilla untouched)

- `World.players: ArrayVec<Player, 2>`. Vanilla has exactly one, and every loop over players with len 1 gives the
  same call order and RNG draws. Sites that read "the player" (aim `Player_AngleFrom` 08 §3.1, graze, hit tests,
  item collection line, `Enemy_ReactPlayerMode`, HUD) take a `PlayerId` from a `Mode`-level policy. In vanilla that
  policy is the constant `PlayerId(0)`, so no new RNG draw or branch reaches vanilla.
- **Co-op** (two players, one playfield): new code under `mode::coop`, with its own spec doc
  (`specs/` addendum or `docs/modes/COOP.md`, marked port-only so spec_refs stays clean). It is parity-free by
  definition.
- **Versus** (two playfields): two `World`s in one `Core`-like `VersusCore`, with cross-field attack events between
  the two `step`s in a fixed order. This works only once there are no globals (F6). That is the strongest
  argument for removing them all, not only the logic ones.
- Each mode has its own `MatchConfig.mode`. Replays of new modes use a port-only .rpy extension section (15 §3
  has user sections), so vanilla .rpy stay readable by the original.

## 2. Mapping from today's src/

| today | goes to | untangle first (measured) |
|---|---|---|
| rng.rs | core::rng | `Rng::boot` wall clock → `BootInput.seed`. Restart reseed (00 §2 step 6) → `NeedRestartSeed` event + input. `TRACE/CALLS` debug → tools feature. |
| bullet.rs `sin_f/cos_f/polar`, d3dx.rs, 17 `atan2`, anmvm ops 63–65, stage `(phi as f64).sin()` | core::fmath | F1–F4. |
| task.rs | core::chain | `Rc<RefCell<dyn Task>>` → `TaskKind` enum + data in World. The RefCell re-entrancy workarounds (`reap`, on-remove timing, task.rs:188) become explicit borrow splits. |
| gamemgr.rs, supervisor.rs | core::flow (+ app for window/launcher) | thread_local `FLAGS`, `RANK_STATE`, `CURRENT` → World. `play_clock`/`run_clock` Instant → host (`PlayClock` events; play time is a score.dat value, 15 §2). `settings::get()` → `CoreFlags`. |
| enemy.rs, eclvm.rs, ecl.rs | core::enemy, core::ecl (VM), formats::ecl | thread_local `TIMER_PAUSE`, `LOAD_FRAMES` → World. `Rc` to bullets/stage/msg → `&mut` borrows passed by the chain runner. 13 gfx refs (draw half) → render side. |
| bullet.rs, item.rs, pshot.rs, player.rs, bomb.rs | core::{bullet,item,player,bomb} | Player `Rc` + `Weak` back-links → `World.players[i]`. bomb `QUEUED` → World. Draw halves → `draw(&self)`. |
| effect.rs | core::effect (+ render) | `EFFECTS`, `LOADING_RELEASE` → World. These effects draw RNG (13 §2), so they are logic. |
| anmvm.rs, anmmgr.rs | core::anm (VM + RNG + speed + screen offset) / render (slots, blend + texture caches, capture) | `AnmManager` split in two (F7). `SLOWDOWN_RESTORE` → World. `ctx.anm.load(&mut gfx, …)` → core keeps sprite/script metadata from formats::anm. The renderer loads textures on demand by `(slot, entry)` id. `entries_loaded` (loading pacing) is counted from data, not from uploads. |
| stage.rs, msg.rs | core + render | 43 gfx refs in stage are camera/fog/draw → render. `Stage_SetCamera` writes cam_right (05 §9 mode 4) → DrawState. |
| hud.rs, ascii.rs, gui.rs | core (popups, gauge, darkness state) + render (`KEPT`, `PANEL` caches) | ascii `POPUPS`/`DARKNESS`/`GAUGE` → World (the gauge reads the player through an id). hud `KEPT`/`PANEL` → DrawState. |
| sound.rs `QUEUE`, audio.rs, midi.rs | core emits `Event::Se/Bgm`, the audio crate plays them | `Sound_Update` (13 §1.3) stays in `step` and turns the queue into events. |
| scoredat.rs, replay.rs, result.rs (save paths) | core builds the bytes (with its RNG draws), host writes them | `storage_write/read` (11 sites) → `Event::Save` / `BootInput.score_dat`. `SAVE_DIR` → host. |
| session.rs, demosync.rs, drawlog.rs, drawreport.rs, bins | th08-tools (+ th08-session for the replay driver) | `Ctx::frame`, `trace_menu`, `sched` debug fields → a tools observer trait. |

## 3. Migration plan (each step verified before the next)

C runs **inside** the baseline order (formats → core → render/audio → tools). Steps C1–C3 are useful even if
netplay is never built: they are also what A needs. "Full check" = verify.zsh sessions + pilot + ref500/ref_b2
(longcheck, run by the orchestrator), and drawgroups for steps that touch draw.

| step | change | check |
|---|---|---|
| C0 | `fmath` module in the current crate. Replace the 17 atan2 sites, tan/acos/atan, sin/cos and `polar` with `libm` f64 → f32. rsqrt table + generator. Clippy disallow list. | Full check (expected identical, F3). New unit tests: table == native on x86_64 (run here). **Cross-target trace test**: the wasm32 build plays `selftest1` + `golden` headless in node and must produce the same trace as Linux. This is the first proof that the wasm build is bit-exact. |
| C1 | **Draw-invariance test** before any refactor: run every session twice, normal and with the draw chain disabled (`cfg+0x23` frame skip forced high, or a tools switch that skips `Chain::Draw`), and compare logic traces. Any difference = draw feeds logic → becomes an IMPL/RE item. | Traces equal on all sessions + a 50-replay sample. |
| C2 | Move the 14 logic thread_locals and `gamemgr::current()` into `Ctx`/`Game` fields (no behaviour change, just ownership). | Full check. A test runs two `Ctx` in one thread on two replays, interleaved frame by frame, both traces exact. |
| C3 | Untangle `Rc<RefCell>`: the managers become plain `World` fields. The task chain gets `TaskKind` dispatch. Back-links become ids. Done per subsystem (player → bullets → enemies → stage/hud). | Full check after each subsystem. |
| C4 | Split `AnmManager` and the draw halves: `draw(&self, &mut DrawState)`. Asset metadata leaves `Gfx`. This is the baseline's core/render split, with the extra rule that draw is `&self`. | Full check + drawgroups (draw must stay visually identical, the draw log ideally identical). C1 test again. |
| C5 | IO and clock out: `BootInput`, `Event::{Save,Se,Bgm,PlayClock}`, `CoreFlags` instead of `settings::get()`. `th08-core` builds with `#![no_std]`-like discipline: no `std::time`, `std::fs`, wgpu, winit or cpal in its dependency graph (cargo-deny/`cargo tree` check in CI). | Full check. `cargo tree -p th08-core` shows no platform crates. |
| C6 | `World: Clone` + `Pool` + `save_state/load_state` + `hash`. | **Rollback torture test** (tools): during a replay, at every frame f: save, step k = 1…8 frames, load, step again, and compare Canary and Full hashes with the straight run. Run on the session set + ref500 sample. **Snapshot transplant**: save at a random frame, serialize, load into a fresh `Core`, continue: the trace must be exact. Bench: save+load < 0.5 ms native, < 2 ms wasm. |
| C7 | `th08-session`: replay driver ported onto it, a synthetic two-peer rollback harness (in-process, with simulated delay/loss/jitter, random prediction failures). | 1010 replays fed as "remote" input with 0–8 frames of artificial delay end bit-exact (trace = offline trace). CI on x86_64-linux + wasm32 (node) + aarch64 (qemu-user, available here). |
| C8 | `th08-net` transport + UI (host/join, delay setting, spectate). Port-only, no spec. | Two processes on loopback, then Linux ↔ wasm browser. Desync counter must stay 0 over full-game runs. |
| C9 (optional) | Co-op / versus modes behind `MatchConfig.mode`. | Vanilla full check unchanged. Mode tests are self-consistency only (own golden traces). |

## 4. What C requires of the core

### Decide at the start of the rewrite (expensive to retrofit)

1. **One owned `World`, no globals, no `Rc/RefCell/Weak`, no trait objects holding state.** References between
   objects are slot indices or ids. Everything else (Clone snapshots, hashing, serde, versus, parallel tests)
   follows from this and is impossible without it.
2. **`draw(&self)` + a separate, non-snapshotted `DrawState`.** If draw keeps `&mut World`, rollback resimulation
   without drawing can desync peers that drew different frames (C1 checks today's code for this).
3. **All nondeterminism is an input; all effects are output events.** No clock, file, setting, GPU or audio query in
   core (F9). This includes the save path's RNG draws staying in core.
4. **Pinned math (`fmath`) and the clippy wall from day one** (F1–F4). Retrofitting means auditing 50k lines for
   float calls again.
5. **Fixed-capacity pools that keep the original slot indices**, with a live bitset. This is cheap to choose now and
   painful to change later, because slot order is logic (free-slot search order is spec'd).
6. **Input = per-player `u16` word array (`MAX_PLAYERS` = 2) in `FrameInput`, and player-indexed state
   (`players[0]` in vanilla).** Changing "the player" from a singleton to an array later touches every module.
7. **Immutable shared `Assets` (`Arc`) separate from `World`**, split into logic assets (hashed: ECL, STD, MSG
   timing, ANM scripts, SHT, ...) and presentation assets (textures, fonts, text strings, music). B needs the same split.

### Can be added later (cheap once the above holds)

- `Pool` live-slot `clone_from`, `ArrayVec` call stacks, POD/memcpy snapshots: optimisations only.
- `StateHash` derive, serde of World, snapshot versioning.
- `th08-session`, `th08-net`, WebRTC, relay, spectator UI, input-delay tuning.
- Co-op/versus logic itself, as long as point 6 holds.
- Speculative audio handling (SE dedup on rollback), BGM sync policy.

## 5. Compatibility, risks, effort

**With A (idiomatic core).** Mostly the same direction: A wants ownership instead of shared state, newtypes, clear
modules. C adds hard constraints A might otherwise choose differently:

- enum task dispatch, not `Box<dyn Task>`;
- `&self` draw;
- no `Rc` even where it would be "idiomatic enough";
- `fmath` as the only float-transcendental entry point;
- `Clone` on everything in World.

A's typed angles and fixed-point newtypes fit: the newtype methods simply call `fmath`. Possible conflict: if A
picks generational arenas (`slotmap`), they must keep the original slot indices and search order (point 5). Use
A's types, with C's index rule.

**With B (content/mod platform).** Shared requirement: the logic/presentation asset split (point 7). The
`MatchConfig` must carry a hash of the *logic* assets after overlays, so two peers with different translation
packs can still play while a gameplay-changing pack is detected. Text replacement must not feed logic. Check
whether any text length reaches logic timing: MSG timing is opcode-driven (10), and the HUD/spell name only
draws; confirm in B. B's hot reload is disabled during a match, or limited to presentation assets. The GDI-text
SPEC-GAP is presentation-only and does not affect C.

**Risks.**

- C0 might change a replay result where glibc's correctly-rounded atan/acos differ from libm-f64 (2 + 2 inputs
  measured). The full check decides. If a session uses one of them, the spec model (x87 fpatan, 05 §10) decides,
  not the trace.
- Draw-feeds-logic cases surfacing in C1. If found, each one is a real port-structure finding (RE question if the
  spec is silent).
- The rsqrt table models one CPU, the one the sessions were recorded on (05 §10). Replays recorded by users on Intel
  machines running the *original* would still differ. That is inherent and unchanged.
- wasm performance of resimulation: unknown per-frame calc cost. Measure in C6. If 8-frame rollback does not fit
  in 16 ms on wasm, use input delay 2–3 + a smaller rollback window.
- Scope creep: C8/C9 are port-only features. Keep them after the rewrite is complete and verified.

**Effort (IMPL cycles of ≤ 3 deliverables).**

| steps | cycles | note |
|---|---|---|
| C0 | 1 | worth doing now, before the rewrite: fixes wasm parity |
| C1 | 1 | |
| C2 | 1 | |
| C3–C5 | ~8–12 | mostly shared with baseline/A; C's extra rules add ~20 % |
| C6 | 2 | |
| C7 | 2 | |
| C8 | 3–5 | |
| C9 | open-ended | needs design docs for the modes |
