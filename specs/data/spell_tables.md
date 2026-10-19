# Spell-card data tables (th08.exe .data) — VERIFIED (read from the exe, RE-Q17M)

Card ids count from 0; screens print id + 1. All strings are Shift-JIS in the exe, shown here decoded. `　` = U+3000 (full-width space).

## 1. Difficulty card lists (`Spell_DifficultyOf` 0x4144D0, result-viewer card groups)

Pointer table 0x4C67D0, counts 0x4C67E8. `Spell_DifficultyOf(id)` returns the index of the first list (0..4) that contains id, else 5 (ids 205–221 = Last Words).

| list | addr | count | ids |
|---|---|---|---|
| 0 Easy | 0x4c6120 | 42 | 2, 6, 13, 17, 21, 25, 32, 39, 43, 47, 54, 58, 62, 66, 70, 77, 81, 85, 89, 93, 100, 104, 108, 112, 119, 123, 127, 131, 135, 139, 143, 147, 151, 155, 159, 163, 167, 171, 175, 179, 183, 187 |
| 1 Normal | 0x4c61c8 | 49 | 3, 7, 10, 14, 18, 22, 26, 29, 33, 36, 40, 44, 48, 51, 55, 59, 63, 67, 71, 74, 78, 82, 86, 90, 94, 97, 101, 105, 109, 113, 116, 120, 124, 128, 132, 136, 140, 144, 148, 152, 156, 160, 164, 168, 172, 176, 180, 184, 188 |
| 2 Hard | 0x4c6290 | 50 | 0, 4, 8, 11, 15, 19, 23, 27, 30, 34, 37, 41, 45, 49, 52, 56, 60, 64, 68, 72, 75, 79, 83, 87, 91, 95, 98, 102, 106, 110, 114, 117, 121, 125, 129, 133, 137, 141, 145, 149, 153, 157, 161, 165, 169, 173, 177, 181, 185, 189 |
| 3 Lunatic | 0x4c6358 | 50 | 1, 5, 9, 12, 16, 20, 24, 28, 31, 35, 38, 42, 46, 50, 53, 57, 61, 65, 69, 73, 76, 80, 84, 88, 92, 96, 99, 103, 107, 111, 115, 118, 122, 126, 130, 134, 138, 142, 146, 150, 154, 158, 162, 166, 170, 174, 178, 182, 186, 190 |
| 4 Extra | 0x4c6420 | 14 | 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204 |
| 5 all | 0x4c6458 | 222 | 0–221 in order |

Every id 0–204 is in exactly one of lists 0–4 (42 + 49 + 50 + 50 + 14 = 205).

## 2. "Last" list 0x4C6B90 (`Spell_IsLastSpell` 0x414540), count at 0x4C6C3C

43 ids: 10, 11, 12, 29, 30, 31, 51, 52, 53, 74, 75, 76, 97, 98, 99, 116, 117, 118, 143, 144, 145, 146, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 204

Used by: info-panel line 1 ("Last" suffix), `LastWord_ComputeUnlocks` (210: ≥ 15, 219: ≥ 30 of these captured in practice).

## 3. Stage card lists 0x4C6C40 (counts 0x4C6C68) — spell practice rows 0–9

| row | addr | count | ids |
|---|---|---|---|
| 0 | 0x4c6800 | 13 | 0–12 (consecutive) |
| 1 | 0x4c6838 | 19 | 13–31 (consecutive) |
| 2 | 0x4c6888 | 22 | 32–53 (consecutive) |
| 3 | 0x4c68e0 | 23 | 54–76 (consecutive) |
| 4 | 0x4c6940 | 23 | 77–99 (consecutive) |
| 5 | 0x4c69a0 | 19 | 100–118 (consecutive) |
| 6 | 0x4c69f0 | 28 | 119–146 (consecutive) |
| 7 | 0x4c6a60 | 44 | 147–190 (consecutive) |
| 8 | 0x4c6b10 | 14 | 191–204 (consecutive) |
| 9 | 0x4c6b48 | 17 | 205–221 (consecutive) |

## 4. Difficulty name table 0x4C8628 (6 pointers; index = `Spell_DifficultyOf`)

| idx | string |
|---|---|
| 0 | `初月（イージー）` |
| 1 | `三日月（ノーマル）` |
| 2 | `上つ弓張（ハード）` |
| 3 | `待宵（ルナティック）` |
| 4 | `望月（エキストラ）` |
| 5 | `極意（ラストワード）` |

(The two pointers after it, 0x4C8640…, are the top-menu description table: `ゲームを開始します`, … — 12 §2.2.)

## 5. Shot name table 0x4C7F4C (13 pointers; `0x453CFA(shot)`; used by info-panel line 2 and replays)

Each name is padded with U+3000 to 8 full-width cells (16 bytes).

| shot | string |
|---|---|
| 0 | `霊夢＆紫　　　　` |
| 1 | `魔理沙＆アリス　` |
| 2 | `咲夜＆レミリア　` |
| 3 | `妖夢＆幽々子　　` |
| 4 | `博麗　霊夢　　　` |
| 5 | `八雲　紫　　　　` |
| 6 | `霧雨　魔理沙　　` |
| 7 | `アリス・Ｍ　　　` |
| 8 | `十六夜　咲夜　　` |
| 9 | `レミリア・Ｓ　　` |
| 10 | `魂魄　妖夢　　　` |
| 11 | `西行寺　幽々子　` |
| 12 | `全主人公合計　　` |

## 6. Full-width digits 0x4C829C (`Fmt_FullwidthDigits` 0x46D763)

10 pointers to `０`…`９`. `Fmt_FullwidthDigits(value, n)` writes n digits (leading zeros) into a static 64-byte buffer 0x18BDDC8; the info panel uses n = 3.

## 7. Unlock-hint table 0x4C82C8 (18 records × 0x30, ids 204–221)

Record = two lines × {format pointer, 5 × i32}. Line 5 of the info panel = line A, line 6 = line B; each is `sprintf(format, a0+1, a1+1, a2+1, a3+1, a4+1)` (unused ints are 0). Used only for never-attempted ids 204–221 that are not selectable (16 §6.3).

| id | line A (format) | A ints | line B (format) | B ints |
|---|---|---|---|---|
| 204 | `挑戦可能条件：望月（エキストラ）をスペルカード７枚以上取得して` | [0, 0, 0, 0, 0] | `　　　　　　　クリアする。` | [0, 0, 0, 0, 0] |
| 205 | `挑戦可能条件：通常モードFinalBを２キャラ以上クリアする。` | [0, 0, 0, 0, 0] | `　　　　　　　難易度不問。` | [0, 0, 0, 0, 0] |
| 206 | `挑戦可能条件：通常モードFinalBを３キャラ以上クリアする。` | [0, 0, 0, 0, 0] | `　　　　　　　難易度不問。` | [0, 0, 0, 0, 0] |
| 207 | `挑戦可能条件：スペルカードモードでスペルカードを50枚以上取得する。` | [0, 0, 0, 0, 0] | `　　　　　　　全キャラ合計。` | [0, 0, 0, 0, 0] |
| 208 | `挑戦可能条件：通常モードFinalBを４キャラ以上クリアする。` | [0, 0, 0, 0, 0] | `　　　　　　　難易度不問。` | [0, 0, 0, 0, 0] |
| 209 | `挑戦可能条件：No.%.3dを取得する。` | [137, 0, 0, 0, 0] | ` ` | [0, 0, 0, 0, 0] |
| 210 | `挑戦可能条件：スペルカードモードでラストスペルを15枚以上取得する。` | [0, 0, 0, 0, 0] | `　　　　　　　全キャラ合計。` | [0, 0, 0, 0, 0] |
| 211 | `挑戦可能条件：No.%.3d,%.3dを取得し` | [145, 195, 0, 0, 0] | `　　　　　　　かつ、No.%.3dを一度でも見る。` | [204, 0, 0, 0, 0] |
| 212 | `挑戦可能条件：No.%.3d,%.3d,%.3dを一度でも見る。` | [208, 209, 210, 0, 0] | `　　　　　　　取得の必要はない。` | [0, 0, 0, 0, 0] |
| 213 | `挑戦可能条件：No.%.3d,%.3d,%.3d,%.3dを一度でも見る。` | [205, 206, 207, 211, 0] | `　　　　　　　取得の必要はない。` | [0, 0, 0, 0, 0] |
| 214 | `挑戦可能条件：魔理沙（単独使用）で三日月（ノーマル）の` | [0, 0, 0, 0, 0] | `　　　　　　　スペルカードを全て取得。` | [0, 0, 0, 0, 0] |
| 215 | `挑戦可能条件：霊夢（単独使用）で上つ弓張（ハード）以上をクリア。` | [0, 0, 0, 0, 0] | `　　　　　　　最終面はFinalBのみ。` | [0, 0, 0, 0, 0] |
| 216 | `挑戦可能条件：スペルカードモードでスペルカードを120枚以上取得する` | [0, 0, 0, 0, 0] | `　　　　　　　全キャラ合計。` | [0, 0, 0, 0, 0] |
| 217 | `挑戦可能条件：通常モードFinalBを６キャラ以上クリアする。` | [0, 0, 0, 0, 0] | `　　　　　　　` | [0, 0, 0, 0, 0] |
| 218 | `挑戦可能条件：望月（エキストラ）を３キャラ以上クリアする。` | [0, 0, 0, 0, 0] | ` ` | [0, 0, 0, 0, 0] |
| 219 | `挑戦可能条件：スペルカードモードでラストスペルを30枚以上取得する。` | [0, 0, 0, 0, 0] | `　　　　　　　全キャラ合計。` | [0, 0, 0, 0, 0] |
| 220 | `挑戦可能条件：待宵（ルナティック）をクリアする。` | [0, 0, 0, 0, 0] | `　　　　　　　コンティニュー可、使用キャラ不問。` | [0, 0, 0, 0, 0] |
| 221 | `挑戦可能条件：これ以外のラストワードを全て一度見る。` | [0, 0, 0, 0, 0] | `　　　　　　　取得の必要はない。` | [0, 0, 0, 0, 0] |

Numbers printed are int + 1: 209 → "No.138", 211 → "No.146,196" / "No.205", 212 → "No.209,210,211", 213 → "No.206,207,208,212".

## 8. Info-panel format strings (16 §6.3)

- 0x4b9448 line 0: `Ｎｏ．%s　　%s`
- 0x4b942c line 1 (args: owner, difficulty name, "Last" or " "): `使用者  %s　　%s %s`
- 0x4b9440 "Last": `Last`
- 0x4b4cbc not-last suffix: ` `
- 0x4b9404 line 2 (arg: shot name): `取得数/挑戦数[最高点]　%s　全主人公合計`
- 0x4b938c line 3, normal card attempted: `　　%3d/%3d(%3d/%3d)[%.8d]　　%3d/%3d(%3d/%3d)[%.8d]`
- 0x4b9354 line 3, Last Word attempted: `　　%3d/%3d(---/---)[%.8d]　　%3d/%3d(---/---)[%.8d]`
- 0x4b93c4 line 3, never attempted: `　　---/---(---/---)[--------]　　---/---(---/---)[--------]`
- 0x4b9344 line 4: `カードの補足`
- 0x4b9330 lines 5/6 when practice captures[12] = 0: `？？？？？？？？？`
- 0x4b6ea0 unknown name / owner: `？？？？？？？？`
- 0x4b9320 Last Word unlocked but unseen (row text): `挑戦者歓迎中！`
