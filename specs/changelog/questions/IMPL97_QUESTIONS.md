# IMPL97 questions

Answered in [answers_re_q34](../answers_re_q34.md).

1. 12 §5.5 lists HUD messages 2 "Supernatural Border!!", 3 "CherryPoint Max!" and 4 "Border Bonus %7d", but no
   numbered spec names a caller of `Hud_ShowMessage` with these types. Are they issued anywhere in th08 (call sites
   of 0x437E5D with type 2, 3 or 4)? The port has their text / colour / scale but never shows them.
2. 08 §8.5 power item: is `Hud_ShowMessage(0, 1)` inside the "no spell card is running" test together with the
   bullet cancel, or unconditional like the item conversion? The port shows it unconditionally.
