# IMPL68 questions

1. **Option return 5 (15 §1.3 vs 00 §3).** 15 §1.3 says the option handler returns 5 when +0x1E / +0x22 changed and
   "that makes the Supervisor recreate the D3D device"; 00 §3 says calc return 5 stops the chain and makes the runner
   return −1 → exit with error. Which path recreates the device (WinMain on runner −1? a Supervisor flag?), is the
   rest of that frame (VM run, draw) skipped, and which value is compared — the snapshot taken by the top-menu
   item 7 switch (12 §2.2) or another copy? The port compares with that snapshot, keeps running and only applies the
   window mode as its fullscreen setting.
2. **Option idle exit (15 §1.3 "After 3600 idle frames the menu falls back to the top menu").** Which counter (the
   demo timer +0x14430 or another), what resets it (any bit of the polled word?), `>= 3600` or `> 3600`, and is it the
   same as Quit (SE 11, top cursor 7, return 1/5) or different (no SE, range interrupt)? The port: counter reset by any
   polled input, `>= 3600`, leaves exactly as Quit.
3. **Key config idle exit.** "After 3600 frames without raw input": is "raw input" the polled word 0x164D528, the raw
   joystick button array, or both? Same comparison question as 2. The port: polled word or any raw pad button,
   `>= 3600`.
4. **Key config previous button 0x4C868C.** Is it written every frame (−1 when no button is down), or only when a
   button is found? (Decides whether releasing and re-pressing the same button re-assigns it.) Its initial value?
   The port writes it every frame, initial −1.
5. **Key config layout and VMs (12 §2.1 VMs 43–76).** Which VM shows which row label, the two-digit button numbers
   (sprites, positions), the ShotSlow Off/On values, the cursor highlight; which range interrupts the screen sends on
   entry and on Quit (the option screen sends 3 on entry). Also: does the key config accept input from the first
   call (no frame gate like the option screen's ≥ 4)? Does the ShotSlow toggle play an SE? The port prints the rows
   with the ASCII queue and gates nothing.
6. **DirectInput axis range (15 §1.1).** The lX/lY thresholds +0x18/+0x1A default to 600, but the DIPROP_RANGE set on
   the device is not given. The port assumes ±1000. Also: does the DirectInput path read the POV hat?
7. **Button indices.** DirectInput button numbers are device-specific; the port maps standard gamepad buttons in a
   fixed order (south, east, west, north, L1, R1, L2, R2, select, start, L3, R3, d-pad up/down/left/right = 0–15).
   No spec question unless the original enumerates devices in a particular way (first attached device?).
