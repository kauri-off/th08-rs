# IMPL105 questions

Answered in [answers_re_q42](../answers_re_q42.md).

1. 12 §2.2 top menu: "Input timing" says calls 0–8 (sub-state 0) "only render description k … no cursor, no change
   test", and the call-9 sentence lists "cursor, highlight, demo counter and the change test" as sub-state-1 code.
   But "Every frame: all item VMs get the unselected sprite, the cursor item its base sprite" — and the `menu_clr`
   draw log shows the unselected sprites (sprite + 1) from the first drawn menu frame. Does the highlight (sprite and
   the 0xFF404040 locked colour) run in every call, sub-states 0, 2 and 3 included? Port: every call (marker in
   `src/menu.rs` `MainMenu::top`).
2. 15 §1.3 key config: where is the key-config cursor set to 0 — in the option item 8 confirm (before
   `Menu_SetScreen(3)`) or in the key-config entry block? The `options_cfg` trace (row 1888) samples cursor 0 at the
   first key-config call's entry. Port: entry block (marker in `MainMenu::key_config_run`). Same question for the
   option screen's cursor 0 from top item 7 (port: at the item-7 confirm, per 12 §2.2 "cursor 0, sub-state 3").
3. 00 §2 restart: which Supervisor fields does `Supervisor_Register` / `Supervisor_OnRegister` reset (current /
   requested / previous scene +0x158/+0x15C/+0x160, frame counters, the replay / demo flags in GM flags)? Is anything
   else re-initialised on this path (the ASCII manager's queue, `g_Rng`, input words 0x164D528/0x164D530, the
   unchanged-input counter 0x164D53C)? Port: scene and requested := boot; everything else kept (marker in
   `Ctx::restart`).
4. 12 §2 / 15 §3.8 replay screen: "a press zeroes +0x10 in its own call". Does the confirm/cancel gate +0x10 > 9
   apply again after returning to the list from sub-state 2 (Bomb), and do sub-states 2 and 3 have a +0x10 gate of
   their own? Port: the list (sub-state 1) needs +0x10 > 9 after every press; sub-states 2/3 have no gate.
5. 12 §2.2 Option wait (sub-state 3, 29 calls): the port skips cursor, change test and demo counter there. Does the
   top handler in sub-state 3 run anything else besides the in-place `MainMenu_Option` call and the counter test?
