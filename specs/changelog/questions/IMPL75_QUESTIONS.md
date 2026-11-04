# IMPL75 questions

None: audit 75–77 are fully specified by 11 §2.1, §3.1, §3.2 and 10 §5 op 11. No SPEC-GAP added or removed
(111 → 111 in `src/`).

Note: the GameFlags word now lives in a thread-local (`gamemgr::flags()`), and `Game::default()`
zeroes it (GameMgr_Ctor, 11 §2). The trace `flags` column (`demosync.rs` row 21) reads the same word, so bits 7 / 8 /
13 now appear in it during last spells; previously they were missing from the column.
