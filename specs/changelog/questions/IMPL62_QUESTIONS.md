# IMPL62 questions

1. **Type-50 segments (13 §2.6).** The ring-init row says `Bomb_Dissolve` sets segments to 64, and the same row says
   the type-50 update sets 32 every frame (row 50: "ring with 32 segments"). Both cannot hold after the first update.
   The port keeps 32 at init and lets `Bomb_Dissolve` write 64 (no per-frame write). Which one is drawn?
   (`effect.rs` SPEC-GAP in `type_init`.)
2. **0x18B8A2C decrement (11 §7 vs 13 §3).** 11 §7 lists the decrement among the scene-switch steps ("afterwards …
   decrements"); 13 §3 says `Supervisor_Calc` decrements it once per frame while ≠ 0. The port decrements every
   frame after the scene switch. Is the decrement inside the `requested ≠ current` block or every frame?
3. **Type 4 and 0x18B8A2C (13 §3).** Type 4 is "as 2", so the port also removes it while 0x18B8A2C ≠ 0
   (dialogue op 14 fade). Confirm that type 4 uses the same calc 0x45B760.
4. **List C camera bit (13 §2.4).** The port projects list C through `Camera_Set2D` even if the effect VM has
   flag bit 15 (stage camera), since `DrawBillboard` uses the transforms current at prio 12. Confirm the VM bit does
   not change the projection there.
