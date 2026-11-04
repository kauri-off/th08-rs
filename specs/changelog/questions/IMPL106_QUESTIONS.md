# IMPL106 questions

Answered in [answers_re_q42](../answers_re_q42.md).

1. 15 §1.3 key config: where in `MainMenu_KeyConfig` 0x469636 is the digit / ShotSlow update 0x469FA9 called —
   before or after the assign, ShotSlow toggle and Reset tests (does a Reset or a new button show in its own call or
   one call later), and does it run in the Quit / idle-exit call? Port: after Reset, not in the Quit call (marker in
   `MainMenu::key_values`).
2. 16 §4: `Info_Show` polls g_InputRawCur itself. Does that `Input_Poll` include the joystick bits (15 §1.1) and the
   ShotSlow counter 0x164D51C, and does it touch the unchanged-input counter 0x164D53C / repeat flag? Port: raw
   keyboard | joystick bits, counter and flag untouched.
3. 12 §2.2: are the 14 description VMs' sprites set by the loader to text.anm sprite 21 + i (as the port does), and
   is anything else (position, colour) written to them outside script 9?
