# GLM vs DeepSeek comparison

## Phase 1 (first pass, Hopper)
| topic | GLM | DeepSeek |
|---|---|---|
| archive | cipher + LZSS found, header mapping wrong, extractor broken | fully working, 317 files |
| bgm | — | table verified, loop field inferred |
| anm/thtx | — | header verified, pixel fmt partial |

## Phase 1 resume (2026-10-17)
- Both subagent runs ended with provider "Rate limit exceeded" (DeepSeek had 3 parallel sub-agents). User: no new subagents for now.
- DeepSeek: archive/file index/extractor done; BGM loop_start (+0x18, relative to track start, bytes) confirmed by sub/bgm draft;
  THTX fmt table 1=A8R8G8B8, 2=A1R5G5B5, 3=R5G6B5, 4=R8G8B8, 5=A4R4G4B4.
- DeepSeek NOTES errors found by my empirical check on ascii.anm: ANM chunk header is 0x40 (not 0x3C; +0x3C is u32 0),
  script table entries are {i32 id; u32 offset} (notes said {offset; id}).
- GLM: only 01_archive.md written.
