# IMPL42 questions

Answered in [answers_re_q30](../answers_re_q30.md).

No new spec questions. Notes for RE:

1. **Golden stops at 9046 (first diff 9047, port player dies).** [RE-Q28](../answers_re_q28.md) attributes this death to CODE BUG C (chained
   damage forwarded to the root, 07 §10.1 step 6), outside IMPL42. Fullrun_noclip 9345 matches [RE-Q28](../answers_re_q28.md)'s predicted
   9346 (death effects, CODE BUG J).
2. **Op 90–92 link position (07 §10.5) vs port.** The spec links a chained child that spawns directly after itself
   (overwriting its own +8); `eclvm.rs` always walks to the tail from the spawner's +8. Same result for a root or a
   spawner without children; differs only for a chained child that already has a successor (spec: no data case known).
   Not changed here (no deliverable); worth an audit row if not already tracked.
3. **Port artefact left:** during an enemy's own `Ecl_Run`, if a neighbour is unlinked (e.g. killed by that run), the
   slot placeholder gets the new links but the running enemy's own copy is updated only at `put`; a var 10096 read
   later in the same run would still use its old +8. No trace case seen.
