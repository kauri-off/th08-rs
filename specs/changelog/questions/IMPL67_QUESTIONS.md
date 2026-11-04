# IMPL67 questions

1. **Supervisor +0x154 config re-check (11 §7).** The spec says the counter +0x154 increments and "every 4000 frames
   a periodic check of the config file may quit", nothing more. Needed: does +0x154 increment every Supervisor calc
   or only on a scene switch; what is checked every 4000 frames (re-read th08.cfg? compare with memory? size /
   version?); what "quit" does (requested = −1, error exit, message). Kept as a SPEC-GAP in `supervisor.rs`.
2. **thbgm.dat message box 0x437CDD (02 §1).** Text and timing of the box shown when thbgm.dat is missing and the
   defaults are applied. The port logs a warning.
3. **th08.cfg at exit (15 §1.2).** Is th08.cfg also written when WinMain leaves with an error (calc chain −1, device
   failure), and before or after the score.dat flush? The port writes it after the score.dat flush on a normal quit.
4. **Bad thbgm.dat header with a valid cfg (02 §1).** "If the file exists but the header mismatches, startup fails" —
   is the header checked on every boot, or only when the defaults are applied? The port checks it on every boot. Also:
   a file shorter than 16 bytes counts as present with a bad header?
