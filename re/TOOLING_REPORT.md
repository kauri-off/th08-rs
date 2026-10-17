# RE tooling report (RE_TOOLING_BRIEF deliverables 1–3)

Date: 2026-10-17. Ghidra 12.1.2 (/opt/ghidra), rea-agents 6.5.0.

## 1. Always-fresh dump (`tools/redump.sh`): done in phase 6

- Runs ApplySymbolsOpus, then DumpAll2. Outputs: `extracted/all_decomp.c`, `callgraph.tsv`, `globals_xref.tsv`. Takes about 50 s.
- Current dump: 2254 functions, **1306 still `FUN_`** (1322 after the first run). symbols.csv has 617 func, 25 global.
- `globals_xref.tsv` only sees absolute-address operands. It does not list struct-relative accesses (`g_Stage+0x625C` and similar).

## 2. rea pilot

### Setup and timings

| step | result |
|---|---|
| `open_binary` (provider `ghidra`) | instant, but lazy |
| first query (`address_name`) | **~41 s**: rea imports the exe into a new ephemeral project and runs full default auto-analysis. It does **not** use `re/ghidra/th08.gpr`. |
| `set_addresses_names` with all 641 symbols.csv entries | all 641 returned `true`, including non-entry labels such as `Music_Play` 0x447D56 (rea created functions or labels there). About 70 s wall, mostly spent emitting the 19 KB JSON payload. The response echoes the map twice (~40 KB of context). |
| `procedure_pseudo_code` by **name** (`Player_DamageAgainst`) | ~0.1 s |
| `batch_decompile` (3 functions) | <0.1 s |
| `xrefs` 0x425430 | <0.1 s, 93 call sites |
| `search_bytes` (whole image) | about 1 s, each match tagged with its containing function name |
| `get_call_graph` backward | instant; returns addresses only (no names); stops at indirect calls (func-chain) |
| compare: `gh.sh Xrefs.java` | 3.5 s per call; `fn.sh` 0.03 s |

### Names stick?
Yes, but only within the session. Renamed callees appear by name in later pseudocode (`Effect_Spawn`, `Vec2_Rotate`, `ItemMgr_Spawn`, `g_GameSpeed`), and procedures can be addressed by name. Names are lost on `close_binary` or when the MCP server restarts. The next session must re-import (~41 s) and re-apply the names (~70 s).

### Quality vs `gh.sh` / `all_decomp.c`
rea's decompile is **worse** than our project, because our project has had `FixThiscall.java` applied and rea's fresh import has not:
- `this` appears as `in_ECX`, and fastcall `EDX` appears as `in_EDX` (e.g. `PlayerBullet_HitSpark8`).
- Calls to thiscall helpers lose their arguments. `FUN_0040b460(&local_34)` becomes `FUN_0040b460()`, and `FUN_00451ce0(&box, pos, size)` loses its out-pointer. The locals it fills then look uninitialized, which makes the box-overlap test in `Player_DamageAgainst` hard to read.
- No signatures from symbols.csv are applied. `set_addresses_names` sets names only.
The control flow and constants were identical. Both versions show the same `Removing unreachable block 0x451C54` warning.

### Re-check of answers_impl9 Priority 1 (type-3 hit spark): CONFIRMED, now stronger
Using rea only (`procedure_pseudo_code`, `xrefs`, `search_bytes`, `get_call_graph`):
- The spawn condition matches the answer. Area +0x3C must be nonzero, `life(+0x24) % period(+0x38) == 0`, and the area must overlap. Then if +0x3D == 0, increment the u8 counter player+0xE2A94; when `(counter & 3) == 0`, spawn `Effect_Spawn(3, pos, 1, 0xFFFFFFFF)`. The `idx < 0xC0 ? 3 : 5` test is always true inside the loop.
- The only direct caller is `EnemyMgr_Calc` (`get_call_graph` backward).
- **New: exhaustive type audit.** The call pattern is `push type; mov ecx,0x4ECE60; call`. Byte pattern `6A 03 B9 60 CE 4E 00 E8` has exactly 1 match: 0x451C6E in `Player_DamageAgainst`. A cross-check of all 133 `mov ecx,EffectMgr; call` sites (rea pattern hits, decoded with `tools/pe.py`) found 12 sites with a runtime type. None can be 3:
  - `Enemy_DropItems` / `EnemyMgr_Calc`: e+0x3310 (signed byte) and e+0x3311+4. Only ECL op 138 writes these, and op 138 appears **0 times** in all 24 ECL files (`ecl_dis.txt`), so the types are 0 and 4.
  - `Ecl_Run` 0x41D01D / 0x41D174: op 139/140 ids in the data are only 17, 26, 40, 51, 60, 61 and 63.
  - `Ecl_Run` 0x41E5ED (`Effect_SpawnHigh`): aura 0x20+n (≥ 32).
  - `Spell_Start` 0x415602 (`Effect_SpawnFixed`): 39 or 52.
  - The others are draw or list helpers (`EffectMgr_Draw*`, `GetFixed` 9/10), not spawns.
  The original answer is VERIFIED including data-driven paths. No spec change is needed.

### Verdict
- **Use rea for:** fast `xrefs`, `search_bytes` with function attribution, call graphs, and ad-hoc decompiles of a few functions once a session is warm. All are sub-second, compared with 3.5 s per `gh.sh` call.
- **Do not use rea as the main reader.** The output lacks thiscall fixes and signatures, setup costs about 2 min per session, renames are lost, and responses are verbose (each result repeats ~1.5 KB of profile and limitations).
- `all_decomp.c` + `fn.sh` remains the best reader: 0.03 s, has `this`, has names. `gh.sh` remains the authority for anything that needs the fixed project.

### Runtime trace (not done; wine is not installed)
- Needs `wine` (32-bit, wow64) + d3d8 → `d3d8to9`/DXVK or wined3d. The game uses DirectX 8. It also needs the game folder copied to a scratch dir, because the game writes score.dat, th08.cfg and replays.
- Breakpoints: `gdb` exists (`/usr/bin/gdb`). It could attach to the wine process, or use `winedbg --gdb`, which is missing. A Frida session (rea `start_frida_session`, Frida is listed available) is easier: hook `Effect_Spawn`, `Rng_Next16`, `Player_DamageAgainst` and log arguments per frame.
- `apitrace` (missing) would capture the D3D8 call stream, which is useful for render-state and draw-order questions.
- Best first use: dump RNG state + call counter per frame during a demo replay. That gives a ground truth for replay-sync tests of the port.

## 3. Proposed edits to `docs/briefs/RE_WORKFLOW.md`

```diff
 ## Tools (`re/tools/`)
-- `redump.sh` — apply `re/symbols.csv` to Ghidra, re-export `re/extracted/all_decomp.c` (`===== name @ addr`), `callgraph.tsv`, `globals_xref.tsv` (~50 s). Run after every batch of new symbols.
+- `redump.sh` — apply `re/symbols.csv` to Ghidra, re-export `re/extracted/all_decomp.c` (`===== name @ addr`), `callgraph.tsv`, `globals_xref.tsv` (~50 s). Run after every batch of new symbols and at the start of a run if all_decomp.c is older than symbols.csv.
-- `fn.sh <addr|name>...` — print function(s) from the dump; `fnc.sh` — same plus `_DAT_` constants annotated as f32/i32.
+- `fn.sh <addr|name>...` — print function(s) from the dump (0.03 s; the default way to read code); `fnc.sh` — same plus `_DAT_` constants annotated as f32/i32.
+- Callers/callees: `grep` `extracted/callgraph.tsv` (columns addr, name, callers, callees) before running any Ghidra script. Indirect calls (func-chain, vtables, effect/bomb tables) are not listed.
...
-- REA MCP (`mcp__rea__*`) is also available; its renames are session-scoped, so persist names in symbols.csv.
+- REA MCP (`mcp__rea__*`, provider `ghidra`): a fresh ephemeral import, not `re/ghidra`. The first query takes ~40 s. There is no thiscall fix and no signatures (`this` shows as `in_ECX`, helper args are dropped), so do not quote its pseudocode for struct offsets without checking `fn.sh`.
+  Good for: `xrefs` (<0.1 s vs 3.5 s gh.sh), `search_bytes` (whole image, matches tagged with function name), `get_call_graph`.
+  Names: `set_addresses_names` with a JSON map built from symbols.csv works (641/641) but is lost on close/restart and costs ~40 KB of context. Apply only the names you need, or none.
+- "Only call site / only writer" claims: prove them with a byte-pattern search (e.g. `6A 03 B9 60 CE 4E 00 E8` = push 3; mov ecx,EffectMgr; call), then check every non-constant site against the data dumps (ecl_dis, anm_dis…). A constant-only search is not enough.
```

```diff
 ## Persistent memory (survives compaction)
 ...
+4. `re/TOOLING_REPORT.md` — tool timings/limits; read once per run, not after every compaction.
```
