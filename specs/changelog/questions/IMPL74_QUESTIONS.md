# IMPL74 questions

No new SPEC-GAP. One SPEC-GAP removed: the night-clock fade order (12 §5.7) is now given by 12 §5.1b (VMs run, then the
fade).

1. 12 §5.1b step 1.1 gates the whole boss-bar step (show/hide interrupts on VM impl+0x1FB0, alpha +0x1C ±4, displayed
   life +0x34) on "no dialogue script runs" (impl+0x2181C < 0). `hud.rs` `boss_calc` gates only the displayed-life
   refresh (10 §8.2) and applies show/hide and alpha during dialogue too. Not changed here (outside audit 72–74);
   should this be a new audit row?
