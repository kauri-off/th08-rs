# Documentation

Reference documents for the port. Behaviour itself is described only in the numbered specifications in
[`specs/`](../specs/); everything here is about the port, its decisions and its status.

| Document | Contents |
|---|---|
| [PLAYING.md](PLAYING.md) | for players: downloads, where the game files go, saves, translations, controls |
| [STATUS.md](STATUS.md) | what works, known differences from the original, what is next |
| [DECISIONS.md](DECISIONS.md) | technical choices (libraries, rendering, assets, timing) and why |
| [PARITY_POLICY.md](PARITY_POLICY.md) | what "identical to the original" means for logic and for drawing |
| [FORMATS.md](FORMATS.md) | short reference of the file formats the port reads and writes |
| [parity/](parity/README.md) | draw parity result and the accepted difference classes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | the architecture as built: crates, frame flow, host interface, how determinism is checked |
| [arch/ARCH_SUMMARY.md](arch/ARCH_SUMMARY.md) | design record (historical): the rewrite plan and its day-0 decisions |
| [arch/ARCH_D_MOBILE.md](arch/ARCH_D_MOBILE.md) | design record (historical): the Android and mobile web plan |
| [arch/ASSETS_FORMAT.md](arch/ASSETS_FORMAT.md) | the repacked game-data format (`.pak`, Vorbis BGM) |
| [arch/I18N.md](arch/I18N.md) | translated game text and the multilingual launcher |
| [arch/I18N_STRINGS.md](arch/I18N_STRINGS.md), [arch/I18N_QUESTIONS.md](arch/I18N_QUESTIONS.md) | inventory of displayed strings, open i18n points |
| [research/RE_SPEC_METHODS.md](research/RE_SPEC_METHODS.md) | how other projects document reverse-engineering results |

Elsewhere in the repository:

- [`../CONTRIBUTING.md`](../CONTRIBUTING.md): how to contribute, the clean-room rule, checks and recipes, commit style.
- [`../tools/README.md`](../tools/README.md): every script and tool bin, and who can run it.
- [`../CLAUDE.md`](../CLAUDE.md): code comment rules and the source-of-truth rules (for people and AI agents), and the
  maintainer machine's workflow.
- [`../specs/`](../specs/): the specifications, the only source of truth for behaviour.
- [`../re/README.md`](../re/README.md): the reverse-engineering tools and where the findings live.
- [`../sessions/README.md`](../sessions/README.md): recording and replaying play sessions of the original.
