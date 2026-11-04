# IMPL61 questions

1. 11 §4 step 3 deferred music start (gm+0x3DE28): what sets this field and when (StageStart only when slot 0 was
   loaded, i.e. not on a same-stage reload; or always)? Which music-room track index does its `Music_PlaySlot` mark
   heard? The port sets it whenever StageStart loads slot 0 and marks nothing.
2. Which config does each slow-mode test read — the Supervisor config 0x17CE895 or the gameplay copy `*(GM+4)` +0x25?
   (a) replay recording step 2 (15 §3.2), (b) `Replay_PlayCalc` (15 §3.3), (c) the CATK backup restore in Result
   modes 0x11/0x16/0x12 and ScoreDat_Save mode 2 (15 §2.2, §2.5), (d) Result state 9 entry skip (16 §2.1),
   (e) the save prompt state 10 (15 §3.6), (f) GameMgr_Calc step 10 frame skip. The port reads the gameplay copy for
   (a), (b) and the Supervisor config for (c)–(e).
3. Replay info +0xB4 and HSCR +0x28 ("th08.cfg copy at record / game start"): copied from the Supervisor config or from
   the gameplay copy (which has lives 2/8 and slow 0 for Extra/practice)? The port writes the Supervisor config bytes.
4. Is the human-rate test "manager clock > 15" made on the clock's integer part (the clock is a Timer at EnemyMgr
   +0x9DCED0, ticked by the game speed)? The port's manager clock is an integer frame count.
