//! Builtin display strings: the exe literals that reach `Text_RenderToVm` (10 §6), keyed for translation (I18N §2.3,
//! G0 inventory docs/arch/I18N_STRINGS.md §2). The text is the vanilla original; printf forms are the exe formats.

// One table row: vanilla text (UTF-8 of the cp932 literal, or the exe printf format), exe VA when it is a single
// literal (0 = table row / port-printed), thcrap stringlocs id ("" = none).
#[derive(Clone, Copy, Debug)]
pub struct Def {
    pub text: &'static str,
    pub va: u32,
    pub thcrap: &'static str,
}

const fn def(text: &'static str, va: u32, thcrap: &'static str) -> Def {
    Def { text, va, thcrap }
}

// A builtin string; indices are into the original tables named on each table below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    // Main menu descriptions (12 §2.2): top menu 0–8, option 0–9, key config 0–11.
    MenuTop(u8),
    MenuOption(u8),
    MenuKey(u8),
    // Player bomb names (14 §1.2, §2): `chara` = row of bomb.rs SPECS (Reimu … Yuyuko), `last` = its last-spell slot.
    Bomb { chara: u8, last: bool },
    // 「ディゾルブスペル」 (14 §2.17) and the name-only 「リザレクション」 of ECL native 22 (07 §11.1, §12).
    Dissolve,
    Resurrection,
    // 8 / 9 / 5 full-width '?' (16 §5.2, §6.3, §2.4).
    Unknown8,
    Unknown9,
    Unknown5,
    // Music-room warning line k (16 §5.3 step 3).
    MusicWarning(u8),
    // Spell practice screen 16 headers, screen 17 header, unlocked Last Word row (16 §6.4, §6.5).
    SpHeader,
    SpPercent,
    SpListHeader,
    SpWelcome,
    // Spell practice info lines 0, 1, 2, 4 and their pieces (16 §6.3).
    SpNoLine,
    SpOwnerLine,
    SpLast,
    SpDiff(u8),
    SpScoreLine,
    SpComment,
    Shot(u8),
    // Last Word unlock hint line `line` of card `card` 204–221 (16 §6.3).
    SpHint { card: u8, line: u8 },
    // Result card-list header and play-data lines (16 §2.4).
    ResultCards,
    StatsRunTime,
    StatsPlayTime,
    StatsPlayHeader,
    StatsRow,
    StatsClear,
    StatsContinue,
    StatsPractice,
}

impl Builtin {
    pub fn def(self) -> Def {
        let i = |k: u8| usize::from(k);
        match self {
            Self::MenuTop(k) => def(DESCRIPTIONS[i(k)], MENU_TOP_VA[i(k)], MENU_TOP_ID[i(k)]),
            Self::MenuOption(k) => def(
                OPTION_DESCRIPTIONS[i(k)],
                MENU_OPTION_VA[i(k)],
                MENU_OPTION_ID[i(k)],
            ),
            Self::MenuKey(k) => def(KEY_DESCRIPTIONS[i(k)], MENU_KEY_VA[i(k)], MENU_KEY_ID[i(k)]),
            Self::Bomb { chara, last } => {
                let k = usize::from(last);
                def(
                    BOMB_NAMES[i(chara)][k],
                    BOMB_VA[i(chara)][k],
                    BOMB_ID[i(chara)][k],
                )
            }
            Self::Dissolve => def("「ディゾルブスペル」", 0x4B_43D4, "th08 Spell Dissolve"),
            Self::Resurrection => def("「リザレクション」", 0x4B_495C, "th08 Spell Resurrection"),
            Self::Unknown8 => def(UNKNOWN8, 0x4B_6EA0, "th08_????????"),
            Self::Unknown9 => def(UNKNOWN9, 0x4B_9330, "th08_?????????"),
            Self::Unknown5 => def(UNKNOWN5, 0x4B_77D8, ""),
            Self::MusicWarning(k) => def(
                WARNING[i(k)],
                MUSIC_WARNING_VA[i(k)],
                MUSIC_WARNING_ID[i(k)],
            ),
            Self::SpHeader => def(
                "ステージ    取得/挑戦可能/総数（()内は全キャラ合計）",
                0x4B_92C0,
                "th08 Spell Practice Description",
            ),
            Self::SpPercent => def(
                "取得率　（キャラ別／総合）",
                0x4B_92A4,
                "th08 Spell Practice Percentages",
            ),
            Self::SpListHeader => def(
                "取得数/挑戦数（()内は本編での集計）",
                0x4B_92FC,
                "th08 Spell Practice List Description",
            ),
            Self::SpWelcome => def("挑戦者歓迎中！", 0x4B_9320, ""),
            Self::SpNoLine => def(
                "Ｎｏ．%s　　%s",
                0x4B_9448,
                "th08_spell_description_string_format",
            ),
            Self::SpOwnerLine => def(
                "使用者  %s　　%s %s",
                0x4B_942C,
                "th08 Spell Practice Owner/Difficulty Line",
            ),
            Self::SpLast => def("Last", 0x4B_9440, "th08 Spell Practice Last Spell"),
            Self::SpDiff(k) => def(DIFF_NAMES[i(k)], SP_DIFF_VA[i(k)], SP_DIFF_ID[i(k)]),
            Self::SpScoreLine => def(
                "取得数/挑戦数[最高点]　%s　全主人公合計",
                0x4B_9404,
                "th08 Spell Practice Highscore Title",
            ),
            Self::SpComment => def("カードの補足", 0x4B_9344, "th08 Spell Practice Comment"),
            Self::Shot(k) => def(
                SHOT_NAMES[i(k)],
                0x4B_732C - 0x14 * u32::from(k),
                SHOT_ID[i(k)],
            ),
            Self::SpHint { card, line } => def(HINTS[i(card) - 204][i(line)], 0, ""),
            Self::ResultCards => def(
                "%3d/%3d（キャラ切り替え↓↑）",
                0x4B_795C,
                "th08 Spell Result Character Select",
            ),
            Self::StatsRunTime => def(
                "総起動時間   %.2d:%.2d:%.2d",
                0x4B_7A9C,
                "th08 Stats Time Since Startup",
            ),
            Self::StatsPlayTime => def(
                "総プレイ時間 %.2d:%.2d:%.2d",
                0x4B_7A80,
                "th08 Stats Total Playtime",
            ),
            Self::StatsPlayHeader => def(
                "プレイ回数　　　 　Easy 　Norm 　Hard 　Luna  Extra  Total",
                0x4B_7A44,
                "th08 Stats Play Count",
            ),
            Self::StatsRow => def(
                "%s %6d %6d %6d %6d %6d %6d",
                0x4B_7A28,
                "th08 Stats Character Format",
            ),
            Self::StatsClear => def("クリア回数  　　", 0x4B_79FC, "th08 Stats Clear Count"),
            Self::StatsContinue => def("コンティニュー  ", 0x4B_79D0, "th08 Stats Continue"),
            Self::StatsPractice => def("プラクティス　  ", 0x4B_79A4, "th08 Stats Practice"),
        }
    }

    pub fn text(self) -> &'static str {
        self.def().text
    }

    // Stable translation key, e.g. `menu.top.3`, `bomb.2.last`, `spell.hint.207.1`.
    pub fn key(self) -> String {
        match self {
            Self::MenuTop(k) => format!("menu.top.{k}"),
            Self::MenuOption(k) => format!("menu.option.{k}"),
            Self::MenuKey(k) => format!("menu.key.{k}"),
            Self::Bomb { chara, last } => {
                format!("bomb.{chara}.{}", if last { "last" } else { "bomb" })
            }
            Self::Dissolve => "bomb.dissolve".into(),
            Self::Resurrection => "spell.resurrection".into(),
            Self::Unknown8 => "unknown8".into(),
            Self::Unknown9 => "unknown9".into(),
            Self::Unknown5 => "unknown5".into(),
            Self::MusicWarning(k) => format!("music.warning.{k}"),
            Self::SpHeader => "spell.header".into(),
            Self::SpPercent => "spell.percent".into(),
            Self::SpListHeader => "spell.list_header".into(),
            Self::SpWelcome => "spell.welcome".into(),
            Self::SpNoLine => "spell.no_line".into(),
            Self::SpOwnerLine => "spell.owner_line".into(),
            Self::SpLast => "spell.last".into(),
            Self::SpDiff(k) => format!("spell.diff.{k}"),
            Self::SpScoreLine => "spell.score_line".into(),
            Self::SpComment => "spell.comment".into(),
            Self::Shot(k) => format!("shot.{k}"),
            Self::SpHint { card, line } => format!("spell.hint.{card}.{line}"),
            Self::ResultCards => "result.cards".into(),
            Self::StatsRunTime => "stats.run_time".into(),
            Self::StatsPlayTime => "stats.play_time".into(),
            Self::StatsPlayHeader => "stats.play_header".into(),
            Self::StatsRow => "stats.row".into(),
            Self::StatsClear => "stats.clear".into(),
            Self::StatsContinue => "stats.continue".into(),
            Self::StatsPractice => "stats.practice".into(),
        }
    }

    // Every builtin, for table checks and the G3 importer.
    pub fn all() -> Vec<Builtin> {
        let n = |len: usize| (0..len).map(|k| k as u8);
        let mut v: Vec<Builtin> = Vec::new();
        v.extend(n(DESCRIPTIONS.len()).map(Self::MenuTop));
        v.extend(n(OPTION_DESCRIPTIONS.len()).map(Self::MenuOption));
        v.extend(n(KEY_DESCRIPTIONS.len()).map(Self::MenuKey));
        for chara in n(BOMB_NAMES.len()) {
            v.extend([false, true].map(|last| Self::Bomb { chara, last }));
        }
        v.extend([
            Self::Dissolve,
            Self::Resurrection,
            Self::Unknown8,
            Self::Unknown9,
            Self::Unknown5,
        ]);
        v.extend(n(WARNING.len()).map(Self::MusicWarning));
        v.extend([
            Self::SpHeader,
            Self::SpPercent,
            Self::SpListHeader,
            Self::SpWelcome,
            Self::SpNoLine,
            Self::SpOwnerLine,
            Self::SpLast,
            Self::SpScoreLine,
            Self::SpComment,
        ]);
        v.extend(n(DIFF_NAMES.len()).map(Self::SpDiff));
        v.extend(n(SHOT_NAMES.len()).map(Self::Shot));
        for card in 204..204 + HINTS.len() as u8 {
            v.extend([0, 1].map(|line| Self::SpHint { card, line }));
        }
        v.extend([
            Self::ResultCards,
            Self::StatsRunTime,
            Self::StatsPlayTime,
            Self::StatsPlayHeader,
            Self::StatsRow,
            Self::StatsClear,
            Self::StatsContinue,
            Self::StatsPractice,
        ]);
        v
    }
}

// Top-menu item descriptions (12 §2.2 table).
const DESCRIPTIONS: [&str; 9] = [
    "ゲームを開始します",
    "エキストラステージを開始します",
    "敵にスペルカード戦をお願いします",
    "ステージを選択し、練習を開始します",
    "リプレイを鑑賞できます",
    "過去のスコアやスペルカードの取得歴を見られます",
    "音楽を聴けます",
    "各種設定できます",
    "いろいろと終了します",
];

// Option descriptions, table 0x4C8664 (12 §2.2).
const OPTION_DESCRIPTIONS: [&str; 10] = [
    "プレイヤーの初期数を変更します。（初期設定　３）",
    "画面の色数を変更します。３２ＢＩＴだと最も綺麗に表示されます。",
    "ＢＧＭの再生方法を変更します。（初期設定　ＷＡＶ）",
    "ＢＧＭの音量を調整します",
    "効果音の音量を調整します",
    "ウィンドウかフルスクリーンか選択します",
    "弾が多い場面でわざと処理落ちさせます(スコア、リプレイ記録不可)",
    "全て初期設定にします",
    "パッド操作のボタン配置を変更します",
    "おいそれと終了します",
];

// Key-config descriptions, table 0x4C8690 (12 §2.2).
const KEY_DESCRIPTIONS: [&str; 12] = [
    "ショット、決定ボタンを設定します",
    "ボム、キャンセルボタンを設定します",
    "低速移動ボタンを設定します",
    "メッセージスキップボタンを設定します",
    "ポーズボタンを設定します",
    "上移動ボタンを設定します",
    "下移動ボタンを設定します",
    "左移動ボタンを設定します",
    "右移動ボタンを設定します",
    "ショット押しっぱなしで低速移動になるようにします",
    "初期設定に戻します",
    "おおよそ終了します",
];

// Shot names 0x4C7F4C, padded to 8 full-width cells (specs/data/spell_tables.md §5).
const SHOT_NAMES: [&str; 13] = [
    "霊夢＆紫　　　　",
    "魔理沙＆アリス　",
    "咲夜＆レミリア　",
    "妖夢＆幽々子　　",
    "博麗　霊夢　　　",
    "八雲　紫　　　　",
    "霧雨　魔理沙　　",
    "アリス・Ｍ　　　",
    "十六夜　咲夜　　",
    "レミリア・Ｓ　　",
    "魂魄　妖夢　　　",
    "西行寺　幽々子　",
    "全主人公合計　　",
];

// Difficulty names 0x4C8628 for info line 1 (16 §6.3; specs/data/spell_tables.md).
const DIFF_NAMES: [&str; 6] = [
    "初月（イージー）",
    "三日月（ノーマル）",
    "上つ弓張（ハード）",
    "待宵（ルナティック）",
    "望月（エキストラ）",
    "極意（ラストワード）",
];

// Unlock hints 0x4C82C8 for ids 204–221, numbers already printed + 1 (16 §6.3; specs/data/spell_tables.md §7).
const HINTS: [[&str; 2]; 18] = [
    [
        "挑戦可能条件：望月（エキストラ）をスペルカード７枚以上取得して",
        "　　　　　　　クリアする。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを２キャラ以上クリアする。",
        "　　　　　　　難易度不問。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを３キャラ以上クリアする。",
        "　　　　　　　難易度不問。",
    ],
    [
        "挑戦可能条件：スペルカードモードでスペルカードを50枚以上取得する。",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを４キャラ以上クリアする。",
        "　　　　　　　難易度不問。",
    ],
    ["挑戦可能条件：No.138を取得する。", " "],
    [
        "挑戦可能条件：スペルカードモードでラストスペルを15枚以上取得する。",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：No.146,196を取得し",
        "　　　　　　　かつ、No.205を一度でも見る。",
    ],
    [
        "挑戦可能条件：No.209,210,211を一度でも見る。",
        "　　　　　　　取得の必要はない。",
    ],
    [
        "挑戦可能条件：No.206,207,208,212を一度でも見る。",
        "　　　　　　　取得の必要はない。",
    ],
    [
        "挑戦可能条件：魔理沙（単独使用）で三日月（ノーマル）の",
        "　　　　　　　スペルカードを全て取得。",
    ],
    [
        "挑戦可能条件：霊夢（単独使用）で上つ弓張（ハード）以上をクリア。",
        "　　　　　　　最終面はFinalBのみ。",
    ],
    [
        "挑戦可能条件：スペルカードモードでスペルカードを120枚以上取得する",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを６キャラ以上クリアする。",
        "　　　　　　　",
    ],
    [
        "挑戦可能条件：望月（エキストラ）を３キャラ以上クリアする。",
        " ",
    ],
    [
        "挑戦可能条件：スペルカードモードでラストスペルを30枚以上取得する。",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：待宵（ルナティック）をクリアする。",
        "　　　　　　　コンティニュー可、使用キャラ不問。",
    ],
    [
        "挑戦可能条件：これ以外のラストワードを全て一度見る。",
        "　　　　　　　取得の必要はない。",
    ],
];

// 8 and 9 full-width '?' (16 §6.3; the 9-char one is string 0x4B9330).
const UNKNOWN8: &str = "？？？？？？？？";
const UNKNOWN9: &str = "？？？？？？？？？";

// Warning table 0x4C7AAC, shown for a comment of a track neither heard nor playing (16 §5.3); U+3000 is part of the
// strings, line 4 is a single U+3000 (not empty, so rendered).
const WARNING: [&str; 7] = [
    "＊＊＊注意＊＊＊",
    "　この曲はまだ、ゲーム中で流れておりません",
    "　自分でプレイする前に聴きたくない、コメントを見たくない、",
    "　という方は一刻も早くここから立ち去る事をお奨めします。",
    "　",
    "　（なお、ここで決定ボタンを押せば、他の曲と同様、",
    "　　曲の再生とコメント表示が行われますので、注意）",
];

const UNKNOWN5: &str = "？？？？？";

const MENU_TOP_VA: [u32; 9] = [
    0x4B_8F1C, 0x4B_8EFC, 0x4B_8ED8, 0x4B_8EB4, 0x4B_8E9C, 0x4B_8E6C, 0x4B_8E5C, 0x4B_8E48,
    0x4B_8E30,
];
const MENU_TOP_ID: [&str; 9] = [
    "th07 Menu Start",
    "th07 Menu Extra Start",
    "th08 Menu Spell Practice",
    "th07 Menu Practice Start",
    "th07 Menu Replay",
    "th07 Menu Result",
    "th07 Menu Music Room",
    "th07 Menu Option",
    "th07 Menu Quit",
];
const MENU_OPTION_VA: [u32; 10] = [
    0x4B_90D0, 0x4B_9090, 0x4B_905C, 0x4B_9040, 0x4B_9024, 0x4B_8FFC, 0x4B_8FBC, 0x4B_8FA4,
    0x4B_8F80, 0x4B_8F68,
];
const MENU_OPTION_ID: [&str; 10] = [
    "th07 Option Player",
    "th07 Option Graphic",
    "th07 Option BGM",
    "th08 Option BGM Volume",
    "th08 Option SE Volume",
    "th07 Option Window Mode",
    "th07 Option Slow Mode",
    "th07 Option Reset",
    "th07 Option Key Config",
    "th07 Option Quit",
];
const MENU_KEY_VA: [u32; 12] = [
    0x4B_926C, 0x4B_9248, 0x4B_922C, 0x4B_9204, 0x4B_91E8, 0x4B_91CC, 0x4B_91B0, 0x4B_9194,
    0x4B_9178, 0x4B_9144, 0x4B_9130, 0x4B_911C,
];
const MENU_KEY_ID: [&str; 12] = [
    "th07 Key Shot",
    "th07 Key Bomb",
    "th07 Key Slow",
    "th07 Key Skip",
    "th07 Key Pause",
    "th07 Key Up",
    "th07 Key Down",
    "th07 Key Left",
    "th07 Key Right",
    "th07 Key ShotSlow",
    "th07 Key Reset",
    "th07 Key Quit",
];

// Bomb / last-spell names per SPECS row (Reimu, Yukari, Marisa, Alice, Sakuya, Remilia, Youmu, Yuyuko), 14 §2.
const BOMB_NAMES: [[&str; 2]; 8] = [
    ["霊符「夢想妙珠」", "神霊「夢想封印　瞬」"],
    ["境符「四重結界」", "境界「永夜四重結界」"],
    ["恋符「マスタースパーク」", "魔砲「ファイナルスパーク」"],
    [
        "魔符「アーティフルサクリファイス」",
        "魔操「リターンイナニメトネス」",
    ],
    ["幻符「殺人ドール」", "幻葬「夜霧の幻影殺人鬼」"],
    ["紅符「不夜城レッド」", "紅魔「スカーレットデビル」"],
    ["人符「現世斬」", "人鬼「未来永劫斬」"],
    ["死符「ギャストリドリーム」", "死蝶「華胥の永眠」"],
];
const BOMB_ID: [[&str; 2]; 8] = [
    ["th08 Bomb Reimu", "th08 Bomb Reimu Last"],
    ["th08 Bomb Yukari", "th08 Bomb Yukari Last"],
    ["th06 Bomb Marisa B", "th08 Bomb Marisa Last"],
    ["th08 Bomb Alice", "th08 Bomb Alice Last"],
    ["th07 Bomb Sakuya A focused", "th08 Bomb Sakuya Last"],
    ["th08 Bomb Remilia", "th08 Bomb Remilia Last"],
    ["th08 Bomb Youmu", "th08 Bomb Youmu Last"],
    ["th08 Bomb Yuyuko", "th08 Bomb Yuyuko Last"],
];
const BOMB_VA: [[u32; 2]; 8] = [
    [0x4B_43A0, 0x4B_43BC],
    [0x4B_44F4, 0x4B_4508],
    [0x4B_4444, 0x4B_446C],
    [0x4B_43F0, 0x4B_4414],
    [0x4B_44C4, 0x4B_44D8],
    [0x4B_4488, 0x4B_44A0],
    [0x4B_453C, 0x4B_4558],
    [0x4B_4574, 0x4B_4590],
];
const MUSIC_WARNING_VA: [u32; 7] = [
    0x4B_6E68, 0x4B_6E3C, 0x4B_6E00, 0x4B_6DC4, 0x4B_6DC0, 0x4B_6D8C, 0x4B_6D58,
];
const SP_DIFF_VA: [u32; 6] = [
    0x4B_86DC, 0x4B_86C8, 0x4B_86B4, 0x4B_869C, 0x4B_8688, 0x4B_8670,
];
const MUSIC_WARNING_ID: [&str; 7] = [
    "th08 Music Room spoiler 1",
    "th08 Music Room spoiler 2",
    "th08 Music Room spoiler 3",
    "th08 Music Room spoiler 4",
    "th08 Music Room spoiler 5",
    "th08 Music Room spoiler 6",
    "th08 Music Room spoiler 7",
];
const SP_DIFF_ID: [&str; 6] = [
    "th08 Spell Practice Easy Mode",
    "th08 Spell Practice Normal Mode",
    "th08 Spell Practice Hard Mode",
    "th08 Spell Practice Lunatic Mode",
    "th08 Spell Practice Extra Mode",
    "th08 Spell Practice Last Word",
];
const SHOT_ID: [&str; 13] = [
    "th08 Stats Reimu & Yukari",
    "th08 Stats Marisa & Alice",
    "th08 Stats Sakuya & Remilia",
    "th08 Stats Youmu & Yuyuko",
    "th08 Stats Reimu",
    "th08 Stats Yukari",
    "th08 Stats Marisa",
    "th08 Stats Alice",
    "th08 Stats Sakuya",
    "th08 Stats Remilia",
    "th08 Stats Youmu",
    "th08 Stats Yuyuko",
    "th08 Stats Total (All Characters)",
];
