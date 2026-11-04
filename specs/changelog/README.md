# Changelog (evidence, not spec)

This directory is the clean-room evidence trail, not a specification: [`questions/`](questions/) holds the questions
the implementers raised while porting from the specs alone (`IMPLn_QUESTIONS.md`), and the `answers_*.md` files hold
the reverse-engineering answers with their evidence (`answers_implN.md` and `answers_re_qN.md` link to the questions
they answer, and each question file links back). Every fact in an answer was carried into the numbered specs
`specs/00–16_*.md` and `specs/structs.md` ([AUDIT.md](AUDIT.md) tracks that merge); those numbered specs are the only
source of truth, and where an answer and a spec disagree, the spec wins.
