# Draw parity status

Draw parity is closed (2026-11-01). Logic: the verify sessions and the pilot set 22/22, the ref500 replay set 490/490,
ref_b2 498/498.

Draw: every remaining difference group in the batch500 set plus the golden session (set `b`) and the 36 dl3 recordings
(set `d`) falls into an accepted class, listed in [draw_classes_151.tsv](draw_classes_151.tsv) (groups and
recordings × frames per class). Accepted classes: precision (orbit stars, particles, glyph / particle snap ties,
enemy ≤ 0.1 px; 13 §2.6c, 05 §10), loader race (12 §5.9), host FPS digits, the menu after a game, draw-log artefacts
(the Youmu rectangle, mis-paired aura quads with identical logs). The evidence for each class is in
`specs/changelog/answers_re_q64.md` … `answers_re_q71.md`; the behaviour itself is in the numbered specs.
