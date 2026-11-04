# IMPL72 questions

1. 07 §12 natives 4 / 7 / 21 "bullet warp across rectangular zones … with scaling about the centre; bullets reverse
   direction". The port reads each constant group as (x0, x1, y0, y1); the first rectangle is the inner one, the second
   the outer one (they share a centre: (192, 208), (192, 224), (191.5, 207.5)). Every live bullet outside the outer
   rectangle (strict) is moved to centre + (pos − centre) · inner/outer (per axis), its velocity negated and its angle
   += π. Questions: which crossing triggers the warp (leaving the outer one, or entering/leaving the inner one), and
   in which direction is the scale applied? Which bullets (all live ones, a state, a flag)? Is "reverse" vel and/or
   angle and/or speed (+0xD70), and are the ex-command states touched? Is the bullet VM rotation updated?
2. 07 §12 natives 9 / 11 / 25 "wide horizontal player-hit rectangles (590 × 160/128, 240/192, 288/224) at the boss
   with a 12-frame warning phase". What are the two heights of each pair (warning vs hit box, graze vs hit, two
   rectangles)? The port hit-tests 590 × the second height (`Player_HitTest`, centred on the caller's pos e+0x2D34) and
   ignores the first. What counts the 12 frames (the calling context's clock, a native-private timer, the
   enemy timer) and what does the warning phase do (graphics, sound)? The port skips the hit while the calling
   context's clock < 12. Is the centre the calling enemy or the boss slot (0xF54CC0[0])?
3. 07 §12 native 13 "stage tint colour 0xFFC03030": is this `Stage_RequestTint` 0x409160 (06 §10.2 step 3), and
   does it also set the "bomb drew" flag +0x646C (so the tint stays on past `Stage_DrawHigh`)? Or does it write the
   stage overlay colour / something else? The port calls `Stage_RequestTint` only.
4. 07 §12 native 16 "bullets with flag 0x100000 within 64 px of a child whose var 10038 == 0 set its var 10038 = 60
   and copy var 10019": which children (the caller's chain via +8, every enemy with a parent)? "copy var 10019" from
   whom (the caller's main context, the bullet angle/speed)? Distance < 64 or ≤ 64, Euclidean or per axis, to child
   pos +0x2D34 or final pos +0x2D88? Is the bullet flag cleared? The port walks the caller's chain, Euclidean < 64 to
   +0x2D34, copies the caller's var 10019 into the child's main context, leaves the bullet.
5. Use counts of natives 4, 7, 9, 11, 13, 16, 21, 25 in the shipped ECL (07 §12 lists none; 07 §7 gives only the
   totals of ops 136 / 137): could they be added to the §12 table, with the sub and time of each call?
