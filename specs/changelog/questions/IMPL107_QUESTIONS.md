# IMPL107 — questions for the dirty room

Answered in [answers_re_q43](../answers_re_q43.md).

1. **State 9, slow mode vs frame-0 interrupt** (src/result.rs `name_start`): 16 §2.1 says slow mode "skips the entry"
   and goes to 0xF; 16 §2.2 says frame 0 first sends interrupt d + 3 to all 72 VMs. Is the slow-mode test before that
   interrupt (port: yes, no slide-in) or after it (VMs get d + 3, then state 0xF)?
2. **States 12 / 14 confirm vs cancel order** (`save_slot`, `save_overwrite`): 16 §2.4 gives cancel-before-confirm for
   the menu pages 1/3/4/6/7 only. In `Result_SaveReplay` state 12 (slot list) and 14 (overwrite?), which is tested
   first when Shot and Bomb edge on the same frame? Port: confirm first.
3. **Spell_Start checksum test on records without the magic** (src/scoredat.rs, 15 §2.5 / §2.12): the hidden code
   adds 1 to the attempt counters of all 222 records, magic or not. 15 §2.5 says a wrong checksum at `Spell_Start`
   zeroes the counters first. Is that test made for a record whose magic is not `CATK` (port: only for present
   records, so a never-attempted card keeps the +1 and its first attempt makes it 2 in all-shot slot 12)?
