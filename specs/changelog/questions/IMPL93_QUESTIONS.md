# IMPL93 questions

Answered in [answers_re_q34](../answers_re_q34.md).

No new SPEC-GAPs. One known omission (spec is clear, port architecture lacks it):

1. 13 §2.6c: `AnmManager_SetRenderState` also calls `Stage_SetCamera` (and so refreshes +0x63C4) when a drawn VM's
   flag bit 15 differs from the cached +0x24C4. The port has no render-state cache and only refreshes +0x63C4 in
   `Stage_DrawLayer`. Which VMs drawn outside the stage layers carry bit 15 in the shipped data (spell backgrounds?),
   i.e. is this path ever reached while the layers are skipped?
