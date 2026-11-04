# impl13 questions for the dirty room

Answered in [answers_impl13](../answers_impl13.md).

1. 12 §5.9 setup order inside `GameManager_StageStart`: does `Ascii_SetupGaugeVms` run before or after the Player registration / `Player_Load` that writes the gauge limits (0x164D300/302)? If it runs before, the first stage of a new game places 人/妖 from the previous game's limits (or the boot defaults). The port runs it after Player_Load (`src/gamemgr.rs`, SPEC-GAP).
2. 12 §5.9 interrupt 99: the excluded requested scenes are given as 3/11/12. 11 §7 (as implemented) has 10 = retry and 11 = reload. Is "retry" 10 or 12, i.e. does a retry from the pause menu hide the readout (99) and only show it again through the fresh-start interrupt 1?
3. `MainMenu_OnRegister` calls the setup with the game manager's current limits. Are those still the last game's limits there (the port uses the default team limits, which only matters while the readout is hidden)?
