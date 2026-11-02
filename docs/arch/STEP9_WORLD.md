# Step 9 — Rc graph → World (plan, IMPL178)

Design: ARCH_A_IDIOMATIC.md §2.3 / §4 step 9, ARCH_SUMMARY decisions 1 and 5, REWRITE_PLAN step 9 (+ mobile rule:
saves outside the original's save points use a World clone, score.dat write draws the RNG). FuncChain semantics:
spec 00 §3, src/task.rs. Rules: behaviour, chain order, RNG call order and every recorded command frozen; one
subsystem per run; each run `tools/arch_check run target/arch_ref` (T D), batch (B) after 9g.

## 1. Holders of game state outside plain fields (master after IMPL177; 9a / 9b columns = after those runs)

| holder (file) | kind | holds | target owner | 9a | 9b |
|---|---|---|---|---|---|
| `PlayerRef` in GameManager, BulletManager, EnemyManager, Stage, Hud, Gui (player.rs) | `Rc<RefCell<Player>>` | Player 0x17D5EF8 | `GameManager.player` | **gone**: `Ctx.player: Option<Box<Player>>` | — |
| `Player.world` (player.rs) | `Weak<RefCell<BulletManager>>` | link to items | parameter `items: Option<&mut ItemManager>` | **gone** | items from `ctx.bullets` in `PlayerTask::calc` |
| ascii `Gauge.player` | `Weak<RefCell<Player>>` | gauge / marker reads | parameter `Option<&Player>` | **gone** | — |
| `BulletManager.enemies` (bullet.rs) | `Weak<RefCell<EnemyManager>>` | time-orb spell bonus | `&mut EnemyManager` at the bullet calc | **gone** (looked up through the GM until 9c) | still through the GM (`BulletTask::calc`) |
| `GameManager.{stage,bullets,enemies,hud}` (gamemgr.rs) | `Option<Rc<RefCell<_>>>` | stage-time subsystems | `GameManager` fields (plain) | 9b–9e | `bullets` **gone**: `Ctx.bullets: Option<Box<BulletManager>>` |
| `EnemyManager.{bullets,stage,msg}` (enemy.rs) | `Rc<RefCell<_>>` | cross links | parameters / `Sys` fields | 9b, 9d, 9e | `bullets` **gone**: `Sys.bullets` (`bm!`, `with_bullets`) |
| `Hud.{msg,enemies,stage}` (hud.rs) | `Rc<RefCell<_>>` | Dialogue owned here, links | `GameManager.msg`; params | 9c–9e | items via `ctx.bullets` (no `enemies.bullets` hop) |
| `Gui.enemies` (gui.rs) | `Rc<RefCell<EnemyManager>>` | spell state | param | 9c | — |
| `Stage.sparkles` (stage.rs) | `RefCell` | draw-phase sparkle points read by the emit | plain field written in `draw_step` (decision 2) | 9d | — |
| `EffectManager` (effect.rs) | `Rc<RefCell<_>>` task | `level` only (state already in `Ctx.effects`) | `World.effects` + Node | 9f | — |
| `ScreenEffect` (effect.rs) | one `TaskRef` per effect | type, alpha, timer | pool `World.screen_fx: Vec<Option<ScreenEffect>>`, Node by slot | 9f | — |
| `ReplayTask` (replay.rs) | `TaskRef`, 4 nodes | playback / record state | `GameManager.replay` | 9g | — |
| `gamemgr::CURRENT` (gamemgr.rs:535) | `thread_local` `Weak<RefCell<GameManager>>` | the running GM, read by player / bullet calc (interim), ascii, replay, demosync, session tools | `World.gm: Option<GameManager>` | 9g | no bullet reads left (player calc, demosync, session use `ctx.bullets`) |
| `Supervisor.scene_task` (supervisor.rs) | `Option<TaskRef>` | GM / MainMenu / Result / Ending / MusicRoom | `World.scene: Scene` enum | 9g | — |
| `AsciiManager`, `AnmDebug`, `Supervisor` | `TaskRef` | menus, debug | `World` fields | 9g | — |
| `Tasks<Ctx>` payload (task.rs) | `Rc<RefCell<dyn Task>>` | every node | `Node` (`Copy` enum) | 9h | — |
| `rng::TRACE` / `CALLS` (rng.rs:85) | `thread_local` | debug call-site log (`TH08_SESSION_RNG`) | `Rng.trace: Option<Vec<String>>` (debug, not compared) | 9h | — |
| `settings::SETTINGS` | `OnceLock` | app/tool settings, debug reads | stays app side; core reads go with step 10 | — | — |
| `Ctx.renderer`, `Ctx.host` | `Box<dyn …>` | renderer, Host | outside World: `Ctx { world, renderer, host, rec, archive, audio }` | 9h | — |

## 2. Cross-subsystem reads: how each becomes a borrow or a parameter

| reader → state | today | after |
|---|---|---|
| any stage subsystem → player | `self.player.borrow*()` | `Sys.player: Option<&mut Player>` (`pl!(sys)`), or `ctx.player` in task fns; `sys.with_player(|p, sys| …)` where a call needs both (the original passes the player and reads globals) — **done 9a** |
| player calc → bullet items, enemy dialogue flag | `Weak` / `CURRENT` | 9a: through the GM (`current()`); 9h: `World.gm` disjoint fields |
| bullet calc → enemy spell bonus | `Weak` | 9a: through the GM; 9c: `&mut EnemyManager` from the caller |
| enemy (ECL) → bullets | `self.bullets.borrow_mut()` | `Sys.bullets: Option<&mut BulletManager>` (`bm!(sys)`, `sys.with_bullets(…)`; None inside the bullet and player calcs) — **done 9b** |
| enemy → stage, msg; hud → stage, enemies, msg; gui → enemies | `Rc` fields | parameters at the task fn (`ctx.<x>` disjoint field borrows) |
| ascii / replay / tools → GM parts | `current()` | `ctx.gm.as_ref()` |

## 3. Chain payload: keep `dyn Task` until the last run

Choice: each run moves one subsystem's state into its owner and leaves a **stateless forwarding task** (unit struct,
like `PlayerTask`) on the chain; the forwarders reach the state through the owner. The chain keeps working unchanged
(order, re-entrancy, deferred on-remove). The last run (9h) replaces `TaskRef` by `Node`: every forwarder becomes one
`Node` variant and `run` dispatches with one `match` — mechanical, because by then no task carries state.
Interim owner: a `Ctx` field (`Ctx.player`, then `Ctx.bullets`, …), since `GameManager` itself is still an `Rc` task;
9g moves those fields into `GameManager` and the GM into `World.gm` in one rename. The player's lifetime equals the
GM's: created by `GameManager::register`, dropped by `Supervisor::drop_scene`.

## 4. Runs

| run | content | files (main) | parallel |
|---|---|---|---|
| 9a (IMPL178) | plan; Player; clone test scaffold (`src/worldclone.rs`) | player, bullet, enemy, eclvm, gamemgr, hud, gui, stage, ascii, game | — |
| 9b (IMPL180, **done**) | BulletManager → `Ctx.bullets` (`BulletTask` forwarder, `Clone`, templates `Arc<[Template]>`); `Sys.bullets`; items reached by the player calc through it; clone test adds bullets + items | bullet, item, enemy, eclvm, gamemgr, hud, player, demosync, bin/session, bin/demosync, game, supervisor, worldclone | no (enemy, eclvm) |
| 9c | EnemyManager → `Ctx.enemies`; ECL `with_taken` named; bullet → enemy and gui/hud → enemy links as params | enemy, eclvm, gui, hud, ascii, replay, bullet, gamemgr | after 9b |
| 9d | Stage → `Ctx.stage`; `sparkles` RefCell → draw-step field | stage, enemy, hud, gamemgr | after 9c; **parallel with 9f** |
| 9e | Hud, Gui, Dialogue → `Ctx` fields; `Hud.msg` / `EnemyManager.msg` links | hud, gui, msg, enemy, ascii, gamemgr | after 9d |
| 9f | EffectManager + ScreenEffect pool in `Ctx.effects` | effect, game, supervisor (loading overlay) | **parallel with 9d** (disjoint files; gamemgr register line only) |
| 9g | GameManager → `World.gm` owning the 9a–9f fields; `CURRENT` removed; ReplayTask; scene tasks (menu, result, ending, music room, ascii, anmdebug) in `World`; supervisor scene enum | gamemgr, supervisor, replay, menu, result, ending, musicroom, ascii, anmdebug, demosync, session | — ; B after |
| 9h | `Node` enum payload + `chain.rs` dispatch; `World: Clone` complete (incl. AnmManager, Arc script data); `rng::TRACE` into `Rng`; `Ctx` = World + IO; clone test covers the whole World incl. a mid-game save from a clone (mobile rule) | task, game, rng, every forwarder, worldclone | — |

Each run extends `src/worldclone.rs` with the moved subsystem (added to its `World`, traced, cloned mid-run).
