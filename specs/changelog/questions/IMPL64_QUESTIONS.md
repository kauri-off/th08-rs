# IMPL64 questions

1. 15 §1.3 option items 0, 1, 2, 5, 6: does a value change play any SE? The port plays SE 0x1D (29) on each change
   of these items (kept from before); 15 §1.3 names SE 0x1D only as the volume preview on items 3/4. The port no
   longer plays an SE on a volume step.
2. 15 §1.3 volume step (b): when Left and Right are both pressed/held on the same frame, are both directions applied
   (Left then Right) or only one? The port applies Left only (step a and step b each test Left first).
3. 15 §1.3 volume preview: does the SE 0x1D preview also run before screen frame 4 (input start) and while the
   leave delay is pending? The port runs it only after the input gate (cursor can only reach items 3/4 there).
