# Visual bugs found by playtesting (our port)

B1. Focused-shot hit effect is perpendicular. Barrier team (Reimu+Yukari), focused fire on Wriggle (stage 1 boss): the hit effect of the focused shot is drawn as long HORIZONTAL streaks (orange/purple) at the boss; it should follow the shot direction. Screenshot: `b1_focus_hit.png` (streaks around x≈600-1100, y≈350-400).

B2. Death sprite never disappears. After the player dies, a red burst sprite stays on screen at the death spot (also visible near the player in `b1_focus_hit.png`, x≈780 y≈1000). Screenshot: `b2_death_sprite.png`.

Unknown whether the cause is in specs or in the implementation.
- Implementer: check src/ against specs; if the code follows the spec, write the question to the current IMPLn_QUESTIONS.md.
- RE: verify the relevant spec sections against th08.exe (shot hit-effect sprite/rotation; player death effect lifetime/deletion), fix the specs in place, answer in `specs/answers_bugs1.md`.
