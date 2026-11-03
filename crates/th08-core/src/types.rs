//! Shared value types of the core (ARCH_A §2.4, STEP11 §1.7): angles, D3DCOLOR, slot indices, scene / difficulty /
//! shot-type enums. Newtypes are `repr(transparent)` and keep the original's raw value; `.0` / `as` at file, trace and
//! ECL-variable boundaries.

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

// Angle in radians, plain f32 arithmetic; each spec'd normalisation is a named fn (`wrap_add` 05 §6.3).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Angle(pub f32);

impl Angle {
    // Angle_AddNormalize (05 §6.3).
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

// Slot index into a fixed pool (D5): the original's slot number, pools keep their order and free-slot search.
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
    // Bullet slot (08 §2).
    BulletIdx(u16);
    // Laser slot (08 §6).
    LaserIdx(u16);
    // Enemy slot (07 §4).
    EnemyIdx(u16);
    // Item slot (08 §8).
    ItemIdx(u16);
    // Effect slot (13 §2).
    EffectIdx(u16);
    // ANM file slot of the AnmManager (03 §5).
    AnmSlot(u8);
    // Sound-effect id (13 §1).
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
    // Team 0–3 of a shot type (09 §2), as `th08_formats::sht::team`.
    pub const fn team(self) -> u8 {
        let s = self as u8;
        if s < 4 { s } else { (s - 4) / 2 }
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
        }
        assert_eq!(ShotType::try_from(12), Err(12));
    }

    #[test]
    fn angle_wrap() {
        assert_eq!(Angle(3.0).wrap_add(1.0).0, wrap_add(3.0, 1.0));
    }
}
