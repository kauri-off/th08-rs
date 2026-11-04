# IMPL65 questions

1. 12 §2.2 +0x6C forwards (8 → 9, 9 → 11, 15 → 16, 16 → 17): which parts of the handler run in the forwarding call
   before `Menu_SetScreen`? The port runs the entry block and the cursor-change VM writes (difficulty 24/25,
   portraits 8/9/23, screen 16's interrupt 18 + pending 27 and scratch VMs), then forwards before any input test.
   Does the forward also play an SE, store config +0x21 (difficulty) or g_ShotType (character)? The port does neither.
2. 12 §2.2: are the range interrupt 5 written by `MainMenu_OnRegister` for the +0x6C redirect and the first handler's
   range interrupt (7 on screen 8, 8 on screen 15) in the same frame, so 5 is never run? The port writes 5 at register
   and the handler overwrites it in the first calc.
3. 12 §2.4 screen 15 cancel (not reachable in normal play, the screen forwards at once): which range interrupt does it
   send, and is it the 30-frame character cancel delay? The port sends 6, flags &= ~1, top menu cursor 2 after 30 frames.
4. 15 §3.8 "+0xC290 … 0 after the menu was entered": is +0xC290 zeroed by the replay screen's entry block, or only
   when the menu object is created (`MainMenu_Register`)? The port keeps it across replay-screen entries within one
   menu object.
5. 15 §3.8 file confirm with no recorded stage: what is the error message (text, position, how long it stays) before
   scene −1? The port still falls back to stage 0.
6. 16 §6.1 shot change: the change test runs at the start of the next handler call in the port (the cursor moves in
   call F, the portraits / +0x1442C reset happen in call F + 1). Is the change test in the original before or after
   `Menu_CursorLeftRight` in the same call?
