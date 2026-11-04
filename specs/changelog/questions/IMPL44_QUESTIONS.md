# IMPL44 questions

Answered in [answers_re_q30](../answers_re_q30.md).

No blocking questions. Notes:

1. **Traces not run (no game data).** golden / fullrun_noclip / selftest1 / demo MIN_SYNC and draw logs need the game data;
   the brief's expected values (demo 0 ~5246, demo 1 ~4155, demo 2 ~4641) are to be confirmed locally, and the
   thresholds raised there.
2. **DropAttracted placement (09 §6.1).** `Player_Die` has no item manager in the port, so it sets a port-only
   request (`Player::drop_attracted`) that the hit-test caller runs right after the test returns: bullet hit
   (`BulletManager::update_normal`), enemy contact (`EnemyManager` contact test) and lasers (end of `update_laser`,
   before its graze orbs). No item is touched between the hit and the drop in any of these paths, so the order
   matches the original. At the real death it runs directly after the death drops (09 §6.2).
3. **Single-kill gauge timers (07 §10.3a).** Only the `GaugeAdd` is skipped while bombing; the gauge timers
   (0x18B89E0 := 0, 0x18B89C8 := 30) and the collect lock 50 stay unconditional, as the spec lists them as separate
   statements. Please confirm they are not inside `GaugeAdd`.
