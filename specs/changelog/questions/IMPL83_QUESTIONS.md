# IMPL83 questions for the dirty room

Answered in [answers_re_q31](../answers_re_q31.md).

1. **Which sites are `Vec3_FromPolar`?** 05 §10 gives the single-rounding model for `Vec3_FromPolar` 0x4286E0. The port
   now uses it wherever a numbered spec writes `polar(...)`. Other places compute cos·r / sin·r inline in the spec
   text and still round cos/sin to f32 first in the port. Are any of these `Vec3_FromPolar` calls, or inline
   `fsin`/`fcos` + `fmul` (which by 05 §10 also round only once)?
   - ECL op 38 `polar` and op 166 `polar_ys` (07 §5 table);
   - ECL movement mode 1 velocity (07 §8, `eclvm.rs`), and the movement delta at `eclvm.rs` mode 2/3;
   - single-kill time orbs "pos + (cos a·r, sin a·r)" (07 §10.3a step 3);
   - effect directions/orbits in 13 §2 (e.g. `e.dir = (cos a, sin a)·m`), the 14 §2 Sakuya spin-out
     "centre + (sin a, cos a)·radius", laser draw positions (08 §6.3 "origin + (cos, sin)·(tail + L/2)").
2. **Hit / graze boxes before the first `Player_Move`.** 09 §1.1 says the boxes are stored by `Player_Move`; the port
   leaves them at 0 from the zeroed registration until the first move and does not refresh them during
   respawn (state 1/2 skips `Player_Move`, while the respawn sets a new position). Is there any other writer of
   +0x38C / +0x3A4 (respawn, stage start, bomb)? If yes, please add it to 09 §3 / §6.3.
