# IMPL47 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **Tie order across load/save cycles (15 §2.3).** The spec says loading score.dat inserts the file's HSCR entries
   with `ScoreList_Insert`, so equal-score entries end up in reverse file order and each load/save cycle flips them.
   The port parses score.dat once (`GameMgr::load_score`) and keeps the entries in list order, so ties keep their
   stored order (`ScoreDat::ranking`, SPEC-GAP). To match, the port would have to re-parse score.dat at each
   `Result_OnRegister` (modes 0, 1, 2) and rebuild the lists there. Not a spec question; a proposed audit item.
