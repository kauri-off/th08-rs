# RE-Q22 — scripted reference sessions (changelog)

Sessions (README: `specs/data/sessions/README.md` "Scripted reference sessions"): `gameover_save`,
`musicroom_practice`, `options_cfg`. Each replays bit-exact in the original (trace 0 differ; draw logs identical
except the fps digits in `gameover_save`, see README).

Spec sections changed (facts are in the numbered specs; this file only lists them and the evidence):
- 15 §1.3 option table, Vol / S.E.Vol row: edge step ±4 then ±1 (= ±5 per press), ±1 per frame while held from the
  30th unchanged frame, SE 0x1D preview every 50 frames. Evidence: `MainMenu_Option` 0x46837B; `options_cfg`
  (80 → 65 after 3 presses, 100 → 53 after a 1.2 s hold). The old text (±1 per press, repeat ±4) was wrong.
- 15 §1.3 new "Key config (screen 3)" block: row order (Skip 4th), working copy, live Up/Down = −1 while open,
  joystick-only assignment with swap, ShotSlow toggle, Reset table, Quit / idle exit. Evidence:
  `MainMenu_KeyConfig` 0x469636, swap helper 0x46950B, table 0x4C78C8; screen labels from `options_cfg`.
- 12 §2.7: item order and labels (Reset before KeyConfig), lives shown as lives + 1, volume steps, SE 0x1D.
- 12 §2.2: return from a practice game goes to screen 8 / **15** (not 16), then forwards 8 → 9 → 11 / 15 → 16 → 17.
  Evidence: `MainMenu_Top`, `MainMenu_Character`, `MainMenu_Difficulty`, `MainMenu_SpellPracticeChar`;
  `musicroom_practice` trace frames 7887–7889 (menu 0 → 15 → 17). 16 §6.1 points to it.

Checked against the traces and already right: no ranking name entry when rank > 9 (16 §2.2); continue default
"No" in a normal game, "Yes" in spell practice when the card was not captured, "Yes" = retry (12 §4, 11 §4.2);
spell-practice hit → forced dissolve spell and failed card (09 §6.1); result states 0xF → 0x10 → 10 → 12 → 13 → 2
(16 §2.1, §2.6); music room unheard-track warning text and wrap (16 §5.3); locked card buzzer (16 §6.2).

Tooling: `re/tools/session.py` treats the bullets/enemies columns as loader-thread columns on loading rows;
`re/tools/play_session.sh` Xvfb check fixed (`pgrep -A -f 'Xvfb :77'`).
