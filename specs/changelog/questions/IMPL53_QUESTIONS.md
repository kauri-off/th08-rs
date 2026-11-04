# IMPL53 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. 07 §12 natives 26 / 28 / 29 "stage VMs interrupt 2 / 1": which VMs? The port sends them to the two
   spell-background VMs (g_Stage +0xB38 / +0xDDC), the ones native 12 interrupts. Is it those two, the overlay VM
   +0x844 (06 §5), or another set?
2. 07 §12 native 28 "bullets' velocities scaled": what is the factor and which vector is scaled (+0xD50 per-frame
   velocity only, or also the speed +0xD70 / ex-command state)? Native 29 "velocities restored": by the inverse factor
   (which game speed, the current one or 1/arg of the call?) or recomputed from angle and speed? The port does
   vel ×= 1/arg in 28 and vel ÷= the current game speed in 29, for every live bullet.
3. 07 §12 natives 28 / 29 sprites: does native 28 write +0x218 for every live bullet or only for those in 0x60–0x6F?
   Does native 29 restore every live bullet's sprite from +0x218 or only those at 0x6F? Which VM(s) of the bullet
   (main VM 0 only)? The port writes +0x218 for every live bullet and restores every one whose sprite differs.
4. 05 §2 +0x218: the bullet template load (0x433070) writes it — with the template's base sprite or with the colour
   offset applied at spawn (08 §3)? Bullets spawned between natives 28 and 29 get their sprite restored from it. The
   port stores the final (coloured) sprite at spawn.
