//! Shared value types of the core: angles, D3DCOLOR, slot indices, scene / difficulty / shot-type enums.

use std::f32::consts::PI;

// Float32 2π = 6.2831855 (05 §6.3, 07 §12 native 8); same bits as `f32::consts::TAU`.
pub const TWO_PI: f32 = 6.283_185_5;

// Angle_AddNormalize 0x43EDB0 (05 §6.3): at most 18 steps each way, float32 π and 2π, so huge angles stay unwrapped.
pub fn wrap_add(a: f32, d: f32) -> f32 {
    let mut a = a + d;
    for _ in 0..18 {
        if a <= PI {
            break;
        }
        a -= TWO_PI;
    }
    for _ in 0..18 {
        if a >= -PI {
            break;
        }
        a += TWO_PI;
    }
    a
}

// Player_AngleFrom (08 §3.1): atan2 of p − o; exactly π/2 when both differences are exactly 0.
pub fn angle_from(o: [f32; 3], p: [f32; 3]) -> f32 {
    let (dx, dy) = (p[0] - o[0], p[1] - o[1]);
    if dx == 0.0 && dy == 0.0 {
        PI / 2.0
    } else {
        crate::fmath::atan2_f(dy, dx)
    }
}

// Low-byte store of an int into a byte field (`mov byte, al`): same bits as `as u8`, wraps.
pub const fn lo8(v: i32) -> u8 {
    v.to_le_bytes()[0]
}

// Angle in radians, plain f32 arithmetic; each spec'd normalisation is a named fn.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Angle(pub f32);

impl Angle {
    // Angle_AddNormalize.
    #[must_use]
    pub fn wrap_add(self, d: f32) -> Self {
        Self(wrap_add(self.0, d))
    }
}

// D3DCOLOR 0xAARRGGBB; in memory little-endian B, G, R, A (05 §2: VM +0x1F0 B .. +0x1F3 A).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct D3dColor(pub u32);

impl D3dColor {
    pub const fn from_bgra(c: [u8; 4]) -> Self {
        Self(u32::from_le_bytes(c))
    }
    pub const fn to_bgra(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }
    pub const fn a(self) -> u8 {
        self.0.to_le_bytes()[3]
    }
    pub const fn r(self) -> u8 {
        self.0.to_le_bytes()[2]
    }
    pub const fn g(self) -> u8 {
        self.0.to_le_bytes()[1]
    }
    pub const fn b(self) -> u8 {
        self.0.to_le_bytes()[0]
    }
}

// Slot index into a fixed pool: the original's slot number, pools keep their order and free-slot search.
macro_rules! slot_idx {
    ($($(#[$m:meta])* $name:ident($t:ty);)*) => {$(
        $(#[$m])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $t);

        impl $name {
            pub const fn idx(self) -> usize {
                self.0 as usize
            }
        }

        impl From<$name> for usize {
            fn from(i: $name) -> usize {
                i.idx()
            }
        }
    )*};
}

slot_idx! {
    BulletIdx(u16);
    LaserIdx(u16);
    EnemyIdx(u16);
    ItemIdx(u16);
    EffectIdx(u16);
    // ANM file slot of the AnmManager.
    AnmSlot(u8);
    // Sound-effect id.
    SeId(u8);
}

// Supervisor scene numbers (11 §7: +0x158 current / +0x15C requested); −1 quits.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneId {
    Quit = -1,
    Boot = 0,
    Menu = 1,
    Game = 2,
    NextStage = 3,
    Error = 4,
    ResultMenu = 5,
    Result = 6,
    ReplayEnd = 7,
    MusicRoom = 8,
    Ending = 9,
    Retry = 10,
    Reload = 11,
    // `Stage_Advance` + reload; never requested in the code (11 §7).
    Advance = 12,
}

impl TryFrom<i32> for SceneId {
    type Error = i32;
    fn try_from(v: i32) -> Result<Self, i32> {
        Ok(match v {
            -1 => Self::Quit,
            0 => Self::Boot,
            1 => Self::Menu,
            2 => Self::Game,
            3 => Self::NextStage,
            4 => Self::Error,
            5 => Self::ResultMenu,
            6 => Self::Result,
            7 => Self::ReplayEnd,
            8 => Self::MusicRoom,
            9 => Self::Ending,
            10 => Self::Retry,
            11 => Self::Reload,
            12 => Self::Advance,
            _ => return Err(v),
        })
    }
}

// GM +0x30 difficulty (11 §2): 0 Easy .. 3 Lunatic, 4 Extra.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Difficulty {
    Easy = 0,
    Normal = 1,
    Hard = 2,
    Lunatic = 3,
    Extra = 4,
}

impl TryFrom<i32> for Difficulty {
    type Error = i32;
    fn try_from(v: i32) -> Result<Self, i32> {
        Ok(match v {
            0 => Self::Easy,
            1 => Self::Normal,
            2 => Self::Hard,
            3 => Self::Lunatic,
            4 => Self::Extra,
            _ => return Err(v),
        })
    }
}

// Shot type byte 0x164D0B1 (09 §2): 0–3 teams, 4–11 solos (even human, odd youkai).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShotType {
    ReimuYukari = 0,
    MarisaAlice = 1,
    SakuyaRemilia = 2,
    YoumuYuyuko = 3,
    Reimu = 4,
    Yukari = 5,
    Marisa = 6,
    Alice = 7,
    Sakuya = 8,
    Remilia = 9,
    Youmu = 10,
    Yuyuko = 11,
}

impl ShotType {
    // Team 0–3 of a shot type, as `th08_formats::sht::team`.
    pub const fn team(self) -> u8 {
        let s = self as u8;
        if s < 4 { s } else { (s - 4) / 2 }
    }

    // 4–11: one character, no focus swap.
    pub const fn is_solo(self) -> bool {
        self as u8 >= 4
    }

    // Low bit of the raw byte: solo youkai, or team 1 / 3 (09 §6.2 respawn script).
    pub const fn is_odd(self) -> bool {
        self as u8 & 1 == 1
    }

    // Solo human = even ≥ 4, solo youkai = odd ≥ 4 (09 §9.3).
    pub const fn is_solo_human(self) -> bool {
        self.is_solo() && !self.is_odd()
    }

    pub const fn is_solo_youkai(self) -> bool {
        self.is_solo() && self.is_odd()
    }
}

impl TryFrom<u8> for ShotType {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        Ok(match v {
            0 => Self::ReimuYukari,
            1 => Self::MarisaAlice,
            2 => Self::SakuyaRemilia,
            3 => Self::YoumuYuyuko,
            4 => Self::Reimu,
            5 => Self::Yukari,
            6 => Self::Marisa,
            7 => Self::Alice,
            8 => Self::Sakuya,
            9 => Self::Remilia,
            10 => Self::Youmu,
            11 => Self::Yuyuko,
            _ => return Err(v),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pi_is_tau() {
        assert_eq!(TWO_PI.to_bits(), std::f32::consts::TAU.to_bits());
    }

    #[test]
    fn d3dcolor_bytes() {
        let c = D3dColor::from_bgra([0x11, 0x22, 0x33, 0x44]);
        assert_eq!(c.0, 0x4433_2211);
        assert_eq!((c.a(), c.r(), c.g(), c.b()), (0x44, 0x33, 0x22, 0x11));
        assert_eq!(c.to_bgra(), [0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn enum_round_trips() {
        for v in -1..=12 {
            assert_eq!(SceneId::try_from(v).map(|s| s as i32), Ok(v));
        }
        assert_eq!(SceneId::try_from(13), Err(13));
        for v in 0..=4 {
            assert_eq!(Difficulty::try_from(v).map(|d| d as i32), Ok(v));
        }
        for v in 0..12u8 {
            let s = ShotType::try_from(v).unwrap();
            assert_eq!(s as u8, v);
            assert_eq!(s.team(), th08_formats::sht::team(v));
            assert_eq!(s.is_solo_human(), v >= 4 && v.is_multiple_of(2));
            assert_eq!(s.is_solo_youkai(), v >= 4 && v % 2 == 1);
        }
        assert_eq!(ShotType::try_from(12), Err(12));
    }

    #[test]
    fn angle_from_zero_is_half_pi() {
        assert_eq!(angle_from([1.0, 2.0, 0.0], [1.0, 2.0, 5.0]), PI / 2.0);
        assert_eq!(
            angle_from([0.0; 3], [3.0, -4.0, 0.0]),
            crate::fmath::atan2_f(-4.0, 3.0)
        );
    }

    #[test]
    fn lo8_is_low_byte() {
        for (v, b) in [
            (-1, 0xFF),
            (0, 0),
            (127, 127),
            (255, 255),
            (256, 0),
            (-256, 0),
            (0x1234, 0x34),
        ] {
            assert_eq!(lo8(v), b);
        }
    }

    #[test]
    fn angle_wrap() {
        assert_eq!(Angle(3.0).wrap_add(1.0).0, wrap_add(3.0, 1.0));
    }
}
