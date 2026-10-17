# Visual bugs found by playtesting (round 2)

C1. Main menu seam. Top menu with "Practice Start" selected: our port shows a hard horizontal seam/band in the background just under the selected item (red box in `c1_menu_seam_port.png`). The original (`c1_menu_seam_orig.png`) has no seam there.

C2. Character select is missing elements. Original `c2_charsel_orig.png` vs ours `c2_charsel_port.png` (Barrier team, Normal):
- the "——FinalB 挑戦可能——" notice above the team name (right side) is missing;
- the bottom-left difficulty plate (crescent moon "三日月 Normal Level" + description line) is missing.

C3. In-game elements not implemented (original `c3_ingame_orig.png`, stage 1, Reimu team, red boxes):
- small floating sprites near killed enemies: grey/white diamonds and small pink glyph sprites;
- the gauge readout at the bottom-left of the playfield: "人 ——100790—— 妖" bar with a "-75.48%" value above it.

Unknown whether the cause is in specs or in the implementation.
- RE: identify each element in th08.exe/data (which ANM file/script/sprite, which function draws/spawns it, conditions, positions, colours, numbers/format), check whether the specs already describe it, fix specs in place, answer in `specs/answers_bugs2.md` stating per item "spec gap" or "already specified in §…".
- Implementer: after the answers, implement and compare with these screenshots.
