# IMPL86 questions for the dirty room

Answered in [answers_re_q32](../answers_re_q32.md).

## Q1 — ECL op 128: which instruction words are a, b, c, d?

07 §5 op 128 says "raw a, b, c, d (4 words)", but every op 128 in the data has **5** argument words (e.g. ecldata1
`6, 0.3, 0.7, 0.3, 0x43000000`). The port takes a, b, c, d from words 1–4 and ignores word 0 (the old signature
was `raw, f a, f b, f c, i d`). Please confirm in 07 §5 that word 0 is unused and a–d are words 1–4.

## Q2 — op 88 on the caller from a thread: what happens to the main context's ip?

07 §5.4 says op 88 calls the sub in the boss's main context "the same way" as op 52, whose first step is
ctx.ip = next instruction. When the target context is not the running one (another enemy, or the caller's own main
context while a thread runs), the port leaves that context's ip as it is (its own resume point), pushes it (unless
flag A bit 26) and enters the sub. Is the pushed return ip the target context's own ip, or the caller's next
instruction?
