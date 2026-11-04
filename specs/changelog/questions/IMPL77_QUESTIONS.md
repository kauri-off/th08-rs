# IMPL77 questions

1. 14 §2.8 (Sakuya LS) says "SE 6 is still played every window frame that does not return early", but the port
   (`bomb.rs` `bomb_sakuya`) plays SE 6 in the window only for the normal bomb (`if !ls`). Not changed here (outside
   audit 81); please confirm the LS plays SE 6 on every frame 20 ≤ T < 148 before the knife loop, so it can get an
   audit row.
2. 15 §2.9 says play time is added "on continue-menu entry". The port adds it at the top of the first
   `ContinueMenu_Update` call, before the stage-practice / replay / ending early exits. Is the add before those tests,
   and does it also happen on the early-exit paths?
