# Step 8e — draw-phase writes inventory and draw = step + emit (IMPL169)

Plan: REWRITE_PLAN step 8 ("draw fns split into draw-phase step (&mut) + emit (&self)"); ARCH_SUMMARY conflict 1 +
decision 2; ARCH_A D2; ARCH_C decision 2. State before: STEP8_GFX_INVENTORY.md §6–§7.

## 1. Classes

| class | meaning | where it goes |
|---|---|---|
| **L** logic | read by calc / logic later (trace-visible, RNG, SE, game / player / enemy fields) | draw-phase step; also headless, re-sim, emit off |
| **V** VM draw state | persistent AnmVm fields written for the draw (pos, scale, rotation + dirty bits, colour, sprite, world matrix of mode 5 / Draw3DWorld) | step: VMs are World state; later calc / draws read them |
| **Q** draw-phase queue | state another draw node consumes this frame (ASCII queue `ctx.ascii` + its colour, gui `texts`, AnmManager tint / screen offset / stage camera / cam_right, effect `stage_tint`) | step (affects emitted commands of later nodes) |
| **E** emit state | D3D render-state mirrors only draw paths read: AnmManager blend / texture / texture-matrix caches | `Recorder::anm` (`drawlist::AnmCache`), emit side |
| **T** temporary | written and restored inside the same draw (pos ± pos_offset, forced colour) | none: the emit draws a modified copy |

**E decision.** `last_blend` +0x24C0, `bound_tex` +0x24BC, `last_tex_matrix` +0x24C8 (05 §12.5, §12.8) moved from
AnmManager to `Recorder::anm`. No calc path reads them (calc only resets them: Supervisor_Calc, Ending_Init); they are
device-state mirrors like `DeviceState.viewport`. With emit off they keep their last emitted value, exactly what the
original does on a frame-skipped frame (00 §2: no draw chain, no DrawByMode). So they are not World state and are not
restored by rollback (decision 2 "render caches live in the renderer side"). This is a change from the brief's
example ("keep in the step"): keeping them in the step would make no anm-drawing node splittable.

**AnmManager API split** (anmmgr.rs): `prepare(&self, &mut AnmVm, mode)` = the only VM write of a draw (mode 5 world
matrix + dirty bits, behind the same visible / alpha / sprite guards, 05 §9); `emit_mode_z` / `emit_mode` / `emit` /
`emit_3d_world` / `draw_billboard` / `draw_ring_strip` take `&self` + `&AnmVm`; `draw*` = prepare + emit (unconverted
callers unchanged). `blend_for(rec, vm)` is an associated fn on the recorder cache.

## 2. Draw nodes (`Task::draw_step`) and helpers

Cols: node (file, prio/tag) · writes (class) · spec · status.

| node | writes | spec | status |
|---|---|---|---|
| GameMgr_Draw (gamemgr) | none (returns 3 / 1) | 11 §3 | **converted** (step only, no emit) |
| ScreenEffect (effect) | none; `done` → remove | 13 §3 | **converted** (step = remove test, emit = fill rect) |
| EffectMgr_Draw (effect, prio 12) lists A/C/E | T (pos / pos_offset / cam flag around the draw) | 13 §2.4 | **converted** (emit only, VM copies) |
| AnmDebug (tool) | V (mode-5 matrix) | 05 §9 | **converted** (step = `prepare`) |
| Ending draw 0x4298F0 | L fade counter `fade_mode` / `fade_t` / `fade_argb` (0x428910); V | 16 §1.5 | **converted** (step = prepare + `fade_step` → `fade_shown`) |
| MusicRoom_Draw 0x449487 | V row VM pos; Q ascii strings; T title pos / colour (matrix kept) | 16 §5.2 | **converted** |
| Supervisor tag 0 loading 0x445BD4 | L `game.load_prompt` counter (12 §1); Q queue colour, push + immediate drain (`loading_prompt`) | 12 §1 | TODO |
| Supervisor tag 2 loading VMs 0x445D3E | T pos + offset | 12 §1 | TODO (emit-only candidate) |
| Supervisor tag 16 replay fps / slot test | Q ascii push + queue colour | 15 §3 | TODO |
| Stage_DrawLow tag 0 | L sparkle list reset; Q `stage_tint` take → tint; E reset; via `draw_layer`: L `obj_flags \|= 2`, Q `stage_camera`, `cam_right`, V quad VM pos / scale | 06 §10.2, 06 §11 | TODO |
| Stage_DrawHigh tag 1 | as draw_layer; effect list B `disp` / `pull_speed` (L, 13 §2.6c, `Effects::draw_background`); spell mask V scale/rot dirty bits (net); L `sparkle_on`, `bomb_drew` take; Q tint off, `stage_camera = None` | 06 §10.2, 13 §2.6c, 13 §5 | TODO |
| Enemy draw prio 8 / 11 (`draw_layer`, `draw_trail`) | Q tint on/off (frozen); V main / sub VM pos, rotation + ROT_DIRTY; T pos_offset, trail scale / colour | 07 §10.6, 06 §10.2 | TODO |
| BulletMgr_Draw prio 13 | Q tint on/off; V bullet VM pos / rotation / RGB; lasers V body / emitter pos / scale / colour (08 §6.3); items V sprite / colour / pos + L `shown` (08 §8.6); list D emit-only | 08 §1, §6.3, §8.6 | TODO |
| Player draw 9 / 10 | V pshot VM rotation / pos / colour (14 §5); T trail alpha; bomb Q tint + V obj pos / rotation (14 §2); V sprite pos, option pos | 14 §2, §5 | TODO |
| Spell_Draw (gui) | L `bonus` / `capturable` (`read_bonus`, 08 §8.5); Q `texts` drained → Text ResCmds; V VM 10 / 11 / 12 pos + sprite; T VM 1 pos; Q tint on / off | 12 §5.8, 06 §10.2 | TODO |
| Hud_Draw | Q ascii pushes + queue colour (clear panel, panel, popups); V front VM pos, mode-5 matrices (cap / ghosts / tiles); **L** `draw_bar`: SE TIMEOUT / TIMEOUT2 on a timer change + `timer_last`, `popup_large_static` (13 §4); L `panel` / `label_latch` (`label_step`, 12 §5.3) | 12 §5.3–§5.6, 13 §4 | TODO |
| Ascii draw 14 / 20 | **L RNG**: `popups_draw` → `darkness_draw` → `file_set_script` runs a VM (13 §4); V popup / gauge / marker VMs (12 §5.9–§5.10); Q `screen_offset := 0` (12 §5.9); Q queue drained per pass; T pause capture copy | 12 §2.9, §3, §4, §5.9–§5.10, 13 §4 | TODO |
| MainMenu draw | V notice VM init / sprite / pos (12 §2.4); menulist sub-draws: Q ascii pushes, V scratch / header / glyph VM pos + sprite, **L-ish** pie interpolation `lists.pies` (spell_char_draw) | 12 §2, §2.4 | TODO |
| Result draw | T VM pos + offset; Q ascii pushes; L `accordion` (draw_cards) | 16 §2.4–§2.6 | TODO |
| `Ctx::draw` boot presents, InfoNotices::draw, `clear_present_twice` | none (outside the chain) | 12 §1, 16 §4, 16 §1.2 | emit only (dropped by the recorder mute with emit off) |

Corrections to ARCH_A D2's list: `paused = 2` is now written in Ascii **calc** (ascii.rs, 11 §2), and the main-menu
description VM runs in calc (`menu::run_vms`, 12 §2); neither is a draw write any more.

## 3. Mechanism

- `Task::draw_step(&mut self, &mut C, tag) -> Ret` (all writes) then `Task::emit(&self, &C, &mut Recorder, tag)`
  (reads only) per node, in chain order (`task::run`). The recorder is moved out of the context for the emit
  (`Recorder::detached` stand-in), so emit cannot write World state.
- `TaskCtx::recorder()` is the switch: `Ctx` returns `Some(&mut rec)` while `Ctx.emit`; `Ctx::set_emit(false)` →
  steps only. `Recorder.mute` (set by `set_emit`) drops DrawCmds from unconverted steps and from draws outside the chain;
  mirrors (viewport, depth, fog) still update; ResCmds (texture create / free / text / capture) are kept — they are
  resource state, not emit. `TODO(step 8 split)`: the mute goes once every step is converted.
- Checks: `task::tests::draw_step_then_emit`, `game::tests::emit_off_records_nothing` (no DrawCmd with emit off up to
  the menu), `session --no-emit` and the arch_check emit-off items (`n_<item>`: sessions + pilot replayed with emit off,
  trace must equal the reference trace; also in FAST). demosync runs calc only (no draw chain), so it has no option.

## 4. Left — `TODO(step 8 split)` per file group (parallel follow-up runs)

1. stage.rs + effect.rs `draw_background` (L obj_flags / disp / sparkles; Q camera / tint).
2. enemy.rs + bullet.rs (+ item.rs, `draw_lasers`) — V / Q only, no L except item `shown`.
3. player.rs + pshot.rs + bomb.rs.
4. hud.rs + gui.rs (L: SE, popup, panel latch, bonus read).
5. ascii.rs + supervisor.rs (L: RNG in darkness, load prompt counter; queue drain).
6. menu.rs + menulist.rs + result.rs.
Then remove `Recorder.mute`.
