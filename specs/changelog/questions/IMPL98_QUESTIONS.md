# IMPL98 questions

Answered in [answers_re_q34](../answers_re_q34.md).

1. **Boss timer visibility** (12 §5.4): `Hud_DrawBoss` draws the timer from HUD +0x24 in the guarded part, but the
   spec gives no condition for drawing it. Drawing it whenever the bar part runs shows a stale value during some bar
   fade-ins (golden 3697–3759, 13700–13762: the reference draws no timer). What gates the timer text (boss timeout
   ≥ 0, +0x24 ≥ 0, something else)? Port keeps: boss slot 0 exists with timeout e+0x3378 > 0 (`hud.rs` `draw_bar`).
2. **"(≤ 99)"** (12 §5.4): is the value clamped to 99 (port: `min(99)`, golden 12354–12433 shows a timer while the
   value is > 99) or is the text skipped above 99? Please state it in the spec.
3. **+0x24 initial value** (12 §5.1a): `Hud_Load` lists no write to +0x24 / +0x28. Is +0x24 kept across stages /
   continues (port: lives in the enemy manager, 0 on every stage start)?
